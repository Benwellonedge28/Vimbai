"""
Fixed Cost Service CRUD Operations

Costs and analyses move from two module-level lists to Neo4j: :FixedCost via
:OWNS_FIXED_COST and :FixedCostAnalysis via :OWNS_FIXED_COST_ANALYSIS, both
stamped with the caller and the Book. Every read is Book-gated, and an id that
belongs to someone else reads as missing rather than being handed over.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from fixed_cost_service.dependencies import book_id_var
from fixed_cost_service.exceptions import NotFoundError

__all__ = ["NotFoundError"]
from fixed_cost_service.models import FixedCost, FixedCostAnalysis
from neo4j import AsyncSession

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


def _bound(value: Optional[datetime]) -> Optional[datetime]:
    """Read a caller-supplied instant as UTC, so it can be compared with a node's.

    Python's own comparison of a naive and an aware datetime raises, and the
    originals stored naive values with no offset at all.
    """
    if value is None:
        return None
    if value.tzinfo is None:
        return value.replace(tzinfo=timezone.utc)
    return value


def _cost_from_node(n: Dict[str, Any], user_id: str) -> FixedCost:
    return FixedCost(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        cost_name=n["cost_name"],
        cost_code=n["cost_code"],
        amount=float(n.get("amount") or 0),
        cost_behavior=n.get("cost_behavior") or "fixed",
        relevant_range_min=float(n.get("relevant_range_min") or 0),
        relevant_range_max=float(n.get("relevant_range_max") or 0),
        period_type=n.get("period_type") or "monthly",
        committed_cost=bool(n.get("committed_cost")),
        controllable=bool(n.get("controllable")),
        department_id=n.get("department_id"),
        product_id=n.get("product_id"),
        created_at=_as_dt(n.get("created_at")),
    )


def _analysis_from_node(n: Dict[str, Any], user_id: str) -> FixedCostAnalysis:
    return FixedCostAnalysis(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        analysis_period_start=_as_dt(n.get("analysis_period_start")),
        analysis_period_end=_as_dt(n.get("analysis_period_end")),
        total_fixed_costs=float(n.get("total_fixed_costs") or 0),
        committed_costs=float(n.get("committed_costs") or 0),
        discretionary_costs=float(n.get("discretionary_costs") or 0),
        controllable_costs=float(n.get("controllable_costs") or 0),
        non_controllable_costs=float(n.get("non_controllable_costs") or 0),
        fixed_cost_details=json.loads(n.get("details_json") or "[]"),
        created_at=_as_dt(n.get("created_at")),
    )


async def add_cost(
    session: AsyncSession,
    user_id: str,
    *,
    cost_name: str,
    cost_code: str,
    amount: float,
    cost_behavior: str = "fixed",
    period_type: str = "monthly",
    committed_cost: bool = False,
    controllable: bool = True,
    department_id: Optional[str] = None,
    product_id: Optional[str] = None,
) -> FixedCost:
    cost = FixedCost(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        cost_name=cost_name,
        cost_code=cost_code,
        amount=amount,
        cost_behavior=cost_behavior,
        period_type=period_type,
        committed_cost=committed_cost,
        controllable=controllable,
        department_id=department_id,
        product_id=product_id,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:FixedCost {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        cost_name: $cost_name,
        cost_code: $cost_code,
        amount: toFloat($amount),
        cost_behavior: $cost_behavior,
        relevant_range_min: toFloat(0),
        relevant_range_max: toFloat(0),
        period_type: $period_type,
        committed_cost: $committed_cost,
        controllable: $controllable,
        department_id: $department_id,
        product_id: $product_id,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_FIXED_COST]->(x)
    RETURN x
    """
    params = {
        "id": cost.id,
        "user_id": user_id,
        "cost_name": cost.cost_name,
        "cost_code": cost.cost_code,
        "amount": cost.amount,
        "cost_behavior": cost.cost_behavior,
        "period_type": cost.period_type,
        "committed_cost": cost.committed_cost,
        "controllable": cost.controllable,
        "department_id": cost.department_id,
        "product_id": cost.product_id,
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)
    return cost


async def get_cost(session: AsyncSession, user_id: str, cost_id: str) -> Optional[FixedCost]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_FIXED_COST]->(x:FixedCost {{id: $cost_id}})
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
    amount: Optional[float] = None,
    committed_cost: Optional[bool] = None,
    controllable: Optional[bool] = None,
) -> FixedCost:
    cost = await get_cost(session, user_id, cost_id)
    if cost is None:
        raise NotFoundError("Cost not found")

    if amount is not None:
        cost.amount = amount
    if committed_cost is not None:
        cost.committed_cost = committed_cost
    if controllable is not None:
        cost.controllable = controllable

    query = """
    MATCH (x:FixedCost {id: $id})
    SET x.amount = toFloat($amount),
        x.committed_cost = $committed_cost,
        x.controllable = $controllable
    """
    params = {
        "id": cost.id,
        "amount": cost.amount,
        "committed_cost": cost.committed_cost,
        "controllable": cost.controllable,
    }
    await _run(session, query, params)
    return cost


async def list_costs(
    session: AsyncSession,
    user_id: str,
    department_id: str = "",
    product_id: str = "",
    committed: Optional[bool] = None,
) -> List[FixedCost]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_FIXED_COST]->(x:FixedCost)
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
    if committed is not None:
        costs = [c for c in costs if c.committed_cost == committed]
    return costs


async def analyse_costs(
    session: AsyncSession,
    user_id: str,
    analysis_period_start: datetime,
    analysis_period_end: datetime,
    department_id: str = "",
    product_id: str = "",
) -> FixedCostAnalysis:
    costs = await list_costs(session, user_id, department_id, product_id)
    analysis = FixedCostAnalysis(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        analysis_period_start=analysis_period_start,
        analysis_period_end=analysis_period_end,
    )

    for cost in costs:
        analysis.fixed_cost_details.append(
            {
                "cost_id": cost.id,
                "cost_name": cost.cost_name,
                "cost_code": cost.cost_code,
                "amount": cost.amount,
                "committed": cost.committed_cost,
                "controllable": cost.controllable,
            }
        )
        analysis.total_fixed_costs += cost.amount
        if cost.committed_cost:
            analysis.committed_costs += cost.amount
        else:
            analysis.discretionary_costs += cost.amount
        if cost.controllable:
            analysis.controllable_costs += cost.amount
        else:
            analysis.non_controllable_costs += cost.amount

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:FixedCostAnalysis {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        analysis_period_start: datetime($start),
        analysis_period_end: datetime($end),
        total_fixed_costs: toFloat($total),
        committed_costs: toFloat($committed),
        discretionary_costs: toFloat($discretionary),
        controllable_costs: toFloat($controllable),
        non_controllable_costs: toFloat($non_controllable),
        details_json: $details_json,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_FIXED_COST_ANALYSIS]->(x)
    RETURN x
    """
    params = {
        "id": analysis.id,
        "user_id": user_id,
        "start": analysis.analysis_period_start.isoformat(),
        "end": analysis.analysis_period_end.isoformat(),
        "total": analysis.total_fixed_costs,
        "committed": analysis.committed_costs,
        "discretionary": analysis.discretionary_costs,
        "controllable": analysis.controllable_costs,
        "non_controllable": analysis.non_controllable_costs,
        "details_json": json.dumps(analysis.fixed_cost_details),
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)
    return analysis


async def list_analyses(
    session: AsyncSession, user_id: str, period_start: Optional[datetime] = None, period_end: Optional[datetime] = None
) -> List[FixedCostAnalysis]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_FIXED_COST_ANALYSIS]->(x:FixedCostAnalysis)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    analyses = [_analysis_from_node(dict(r["x"]), user_id) async for r in result]
    start, end = _bound(period_start), _bound(period_end)
    if start:
        analyses = [a for a in analyses if a.analysis_period_start >= start]
    if end:
        analyses = [a for a in analyses if a.analysis_period_end <= end]
    return analyses


def summarise(costs: List[FixedCost]) -> Dict[str, Any]:
    return {
        "total_fixed_costs": sum(c.amount for c in costs),
        "committed_costs": sum(c.amount for c in costs if c.committed_cost),
        "discretionary_costs": sum(c.amount for c in costs if not c.committed_cost),
        "controllable_costs": sum(c.amount for c in costs if c.controllable),
        "non_controllable_costs": sum(c.amount for c in costs if not c.controllable),
        "total_cost_items": len(costs),
    }
