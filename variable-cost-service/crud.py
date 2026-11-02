"""
Variable Cost Service CRUD Operations

Costs and analyses move from two module-level lists to Neo4j: :VariableCost via
:OWNS_VARIABLE_COST and :VariableCostAnalysis via :OWNS_VARIABLE_COST_ANALYSIS,
stamped with the caller and the Book. Reads are Book-gated, and the maps an
analysis builds ride along as JSON props.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from variable_cost_service.dependencies import book_id_var
from variable_cost_service.exceptions import NotFoundError
from variable_cost_service.models import VariableCost, VariableCostAnalysis

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _as_dt(value) -> datetime:
    if value is None:
        return _now()
    if isinstance(value, datetime):
        dt = value
    else:
        iso = value.iso_format() if hasattr(value, "iso_format") else str(value)
        if iso is None or iso == "None":
            return _now()
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _cost_from_node(n: Dict[str, Any], user_id: str) -> VariableCost:
    return VariableCost(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        cost_name=n["cost_name"],
        cost_code=n["cost_code"],
        unit_cost=float(n.get("unit_cost") or 0),
        total_cost=float(n.get("total_cost") or 0),
        quantity=float(n.get("quantity") or 0),
        cost_driver=n.get("cost_driver") or "",
        cost_driver_rate=float(n.get("cost_driver_rate") or 0),
        department_id=n.get("department_id"),
        product_id=n.get("product_id"),
        created_at=_as_dt(n.get("created_at")),
    )


def _analysis_from_node(n: Dict[str, Any], user_id: str) -> VariableCostAnalysis:
    return VariableCostAnalysis(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        analysis_period=n.get("analysis_period") or "",
        total_variable_costs=float(n.get("total_variable_costs") or 0),
        total_units=float(n.get("total_units") or 0),
        average_cost_per_unit=float(n.get("average_cost_per_unit") or 0),
        cost_driver_analysis=json.loads(n.get("cost_driver_json") or "{}"),
        product_costs=json.loads(n.get("product_costs_json") or "{}"),
        created_at=_as_dt(n.get("created_at")),
    )


async def add_cost(
    session: AsyncSession,
    user_id: str,
    *,
    cost_name: str,
    cost_code: str,
    unit_cost: float,
    quantity: float,
    cost_driver: str,
    department_id: Optional[str] = None,
    product_id: Optional[str] = None,
) -> VariableCost:
    cost = VariableCost(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        cost_name=cost_name,
        cost_code=cost_code,
        unit_cost=unit_cost,
        quantity=quantity,
        cost_driver=cost_driver,
        # the original sets the driver rate to the unit cost at creation
        cost_driver_rate=unit_cost,
        total_cost=unit_cost * quantity,
        department_id=department_id,
        product_id=product_id,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:VariableCost {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        cost_name: $cost_name,
        cost_code: $cost_code,
        unit_cost: toFloat($unit_cost),
        total_cost: toFloat($total_cost),
        quantity: toFloat($quantity),
        cost_driver: $cost_driver,
        cost_driver_rate: toFloat($cost_driver_rate),
        department_id: $department_id,
        product_id: $product_id,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_VARIABLE_COST]->(x)
    RETURN x
    """
    params = {
        "id": cost.id,
        "user_id": user_id,
        "cost_name": cost.cost_name,
        "cost_code": cost.cost_code,
        "unit_cost": cost.unit_cost,
        "total_cost": cost.total_cost,
        "quantity": cost.quantity,
        "cost_driver": cost.cost_driver,
        "cost_driver_rate": cost.cost_driver_rate,
        "department_id": cost.department_id,
        "product_id": cost.product_id,
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)
    return cost


async def get_cost(session: AsyncSession, user_id: str, cost_id: str) -> Optional[VariableCost]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_VARIABLE_COST]->(x:VariableCost {{id: $cost_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, cost_id=cost_id)
    records = [r async for r in result]
    if not records:
        return None
    return _cost_from_node(dict(records[0]["x"]), user_id)


async def update_cost(
    session: AsyncSession,
    user_id: str,
    cost_id: str,
    *,
    unit_cost: Optional[float] = None,
    quantity: Optional[float] = None,
) -> VariableCost:
    cost = await get_cost(session, user_id, cost_id)
    if cost is None:
        raise NotFoundError("Cost not found")

    if unit_cost is not None:
        cost.unit_cost = unit_cost
    if quantity is not None:
        cost.quantity = quantity
    cost.total_cost = cost.unit_cost * cost.quantity

    query = """
    MATCH (x:VariableCost {id: $id})
    SET x.unit_cost = toFloat($unit_cost),
        x.quantity = toFloat($quantity),
        x.total_cost = toFloat($total_cost)
    """
    params = {
        "id": cost.id,
        "unit_cost": cost.unit_cost,
        "quantity": cost.quantity,
        "total_cost": cost.total_cost,
    }
    await _run(session, query, params)
    return cost


async def list_costs(
    session: AsyncSession, user_id: str, department_id: str = "", product_id: str = ""
) -> List[VariableCost]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_VARIABLE_COST]->(x:VariableCost)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    costs = [_cost_from_node(dict(r["x"]), user_id) async for r in result]
    if department_id:
        costs = [c for c in costs if c.department_id == department_id]
    if product_id:
        costs = [c for c in costs if c.product_id == product_id]
    return costs


async def analyse_costs(
    session: AsyncSession, user_id: str, analysis_period: str, department_id: str = "", product_id: str = ""
) -> VariableCostAnalysis:
    costs = await list_costs(session, user_id, department_id, product_id)
    analysis = VariableCostAnalysis(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        analysis_period=analysis_period,
    )

    for cost in costs:
        analysis.total_variable_costs += cost.total_cost
        analysis.total_units += cost.quantity
        if cost.product_id:
            analysis.product_costs[cost.product_id] = analysis.product_costs.get(cost.product_id, 0) + cost.total_cost
        analysis.cost_driver_analysis[cost.cost_driver] = (
            analysis.cost_driver_analysis.get(cost.cost_driver, 0) + cost.total_cost
        )

    if analysis.total_units > 0:
        analysis.average_cost_per_unit = analysis.total_variable_costs / analysis.total_units

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:VariableCostAnalysis {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        analysis_period: $analysis_period,
        total_variable_costs: toFloat($total),
        total_units: toFloat($units),
        average_cost_per_unit: toFloat($average),
        cost_driver_json: $cost_driver_json,
        product_costs_json: $product_costs_json,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_VARIABLE_COST_ANALYSIS]->(x)
    RETURN x
    """
    params = {
        "id": analysis.id,
        "user_id": user_id,
        "analysis_period": analysis.analysis_period,
        "total": analysis.total_variable_costs,
        "units": analysis.total_units,
        "average": analysis.average_cost_per_unit,
        "cost_driver_json": json.dumps(analysis.cost_driver_analysis),
        "product_costs_json": json.dumps(analysis.product_costs),
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)
    return analysis


def summarise(costs: List[VariableCost]) -> Dict[str, Any]:
    total_units = sum(c.quantity for c in costs)
    total_cost = sum(c.total_cost for c in costs)
    return {
        "total_variable_costs": total_cost,
        "total_units": total_units,
        "average_cost_per_unit": total_cost / total_units if total_units > 0 else 0,
        "cost_drivers": list({c.cost_driver for c in costs}),
        "total_cost_items": len(costs),
    }
