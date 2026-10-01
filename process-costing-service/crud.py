"""
Process Costing Service CRUD Operations

Neo4j-backed persistence for cost calculations. All records are stamped
with book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)`.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from process_costing_service.dependencies import book_id_var
from process_costing_service.exceptions import NotFoundError
from process_costing_service.models import CostCalculation, CostCalculationCreate, CostComponent

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _as_dt(value) -> Optional[datetime]:
    if value is None:
        return None
    if isinstance(value, datetime):
        dt = value
    else:
        iso = value.iso_format() if hasattr(value, "iso_format") else str(value)
        if iso is None or iso == "None":
            return None
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _calc_from_node(n: Dict[str, Any], user_id: str) -> CostCalculation:
    components = [CostComponent(**c) for c in json.loads(n.get("components_json") or "[]")]
    return CostCalculation(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        product_or_process=n["product_or_process"],
        period=n.get("period", ""),
        components=components,
        total_cost=float(n.get("total_cost", 0)),
        unit_cost=float(n.get("unit_cost", 0)),
        quantity=int(n.get("quantity", 1)),
        notes=n.get("notes", ""),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


async def _list_calculations(session: AsyncSession, user_id: str, company_id: str) -> List[CostCalculation]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CALCULATION]->(x:CostCalculation {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    return [_calc_from_node(dict(r["x"]), user_id) async for r in result]


async def create_calculation(session: AsyncSession, user_id: str, payload: CostCalculationCreate) -> CostCalculation:
    calc = CostCalculation(
        id=str(uuid.uuid4()),
        user_id=user_id,
        company_id=payload.company_id,
        product_or_process=payload.product_or_process,
        period=payload.period,
        components=list(payload.components),
        quantity=payload.quantity,
        notes=payload.notes,
    )
    # totals are a pure computation over the submitted components (original semantics)
    calc.total_cost = sum(c.amount for c in calc.components)
    calc.unit_cost = calc.total_cost / max(1, calc.quantity)
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:CostCalculation {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        product_or_process: $product_or_process,
        period: $period,
        components_json: $components_json,
        total_cost: toFloat($total_cost),
        unit_cost: toFloat($unit_cost),
        quantity: toInteger($quantity),
        notes: $notes,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_CALCULATION]->(x)
    RETURN x
    """
    params = {
        "id": calc.id,
        "user_id": user_id,
        "company_id": calc.company_id,
        "product_or_process": calc.product_or_process,
        "period": calc.period,
        "components_json": json.dumps([c.model_dump() for c in calc.components]),
        "total_cost": calc.total_cost,
        "unit_cost": calc.unit_cost,
        "quantity": calc.quantity,
        "notes": calc.notes,
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _calc_from_node(dict(records[0]["x"]), user_id)


async def get_calculations(session: AsyncSession, user_id: str, company_id: str, product: str = "") -> Dict[str, Any]:
    calcs = await _list_calculations(session, user_id, company_id)
    if product:
        calcs = [c for c in calcs if product.lower() in c.product_or_process.lower()]
    return {"company_id": company_id, "calculations": calcs, "total": len(calcs)}


async def get_cost_breakdown(session: AsyncSession, user_id: str, company_id: str, calc_id: str) -> Dict[str, Any]:
    calcs = await _list_calculations(session, user_id, company_id)
    for c in calcs:
        if c.id == calc_id:
            by_type = {}
            for comp in c.components:
                by_type[comp.cost_type] = by_type.get(comp.cost_type, 0.0) + comp.amount
            return {
                "calc_id": calc_id,
                "total": c.total_cost,
                "unit_cost": c.unit_cost,
                "breakdown": by_type,
                "components": c.components,
            }
    raise NotFoundError("Calculation not found")


async def cost_summary(session: AsyncSession, user_id: str, company_id: str) -> Dict[str, Any]:
    calcs = await _list_calculations(session, user_id, company_id)
    if not calcs:
        return {
            "company_id": company_id,
            "total_calculations": 0,
            "total_cost": 0,
            "avg_unit_cost": 0,
        }
    return {
        "company_id": company_id,
        "total_calculations": len(calcs),
        "total_cost": sum(c.total_cost for c in calcs),
        "avg_unit_cost": sum(c.unit_cost for c in calcs) / len(calcs),
    }
