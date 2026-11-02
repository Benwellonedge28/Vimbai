"""
Overhead Absorption Rate Service CRUD Operations

Rates and analyses are nodes, Book-gated, so one caller's overhead register and
working are invisible to another.

QUIRKS preserved:
* a rate is only worked out when the budgeted base is above zero; otherwise both
  the rate and the per-unit rate stay at zero rather than dividing;
* the cost-based bases return a percentage, the hours and unit bases a rate per
  unit, and the analysis record stores whichever was produced;
* an analysis always records the budgeted overhead as overhead absorbed, it is a
  budget working rather than an actual-activity calculation.
"""

import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

from neo4j import AsyncSession
from overhead_absorption_rate_service.dependencies import book_id_var
from overhead_absorption_rate_service.models import OARAnalysis, OverheadAbsorptionRate

RATE_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


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
        if iso in (None, "None"):
            return None
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _rate_from_node(n: Dict, user_id: str) -> OverheadAbsorptionRate:
    return OverheadAbsorptionRate(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        cost_centre_name=n.get("cost_centre_name") or "",
        cost_centre_id=n.get("cost_centre_id") or "",
        overhead_type=n.get("overhead_type") or "production",
        absorption_base=n.get("absorption_base") or "machine_hours",
        budgeted_overhead=float(n.get("budgeted_overhead") or 0),
        budgeted_base_units=float(n.get("budgeted_base_units") or 0),
        absorption_rate=float(n.get("absorption_rate") or 0),
        rate_per_unit=float(n.get("rate_per_unit") or 0),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _analysis_from_node(n: Dict, user_id: str) -> OARAnalysis:
    return OARAnalysis(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        entity_name=n.get("entity_name") or "",
        cost_centre_name=n.get("cost_centre_name") or "",
        rate_type=n.get("rate_type") or "",
        absorption_base=n.get("absorption_base") or "",
        calculated_rate=float(n.get("calculated_rate") or 0),
        base_units_used=float(n.get("base_units_used") or 0),
        overhead_absorbed=float(n.get("overhead_absorbed") or 0),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


async def create_rate(
    session: AsyncSession,
    user_id: str,
    *,
    cost_centre_name: str,
    cost_centre_id: str,
    budgeted_overhead: float,
    budgeted_base_units: float,
    overhead_type: str = "production",
    absorption_base: str = "machine_hours",
) -> OverheadAbsorptionRate:
    rate = OverheadAbsorptionRate(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        cost_centre_name=cost_centre_name,
        cost_centre_id=cost_centre_id,
        overhead_type=overhead_type,
        absorption_base=absorption_base,
        budgeted_overhead=budgeted_overhead,
        budgeted_base_units=budgeted_base_units,
    )
    if budgeted_base_units > 0:
        rate.absorption_rate = budgeted_overhead / budgeted_base_units
        rate.rate_per_unit = rate.absorption_rate

    await _run(
        session,
        """
        MATCH (u:User {id: $user_id})
        CREATE (x:OverheadAbsorptionRate {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            cost_centre_name: $cost_centre_name,
            cost_centre_id: $cost_centre_id,
            overhead_type: $overhead_type,
            absorption_base: $absorption_base,
            budgeted_overhead: toFloat($budgeted_overhead),
            budgeted_base_units: toFloat($budgeted_base_units),
            absorption_rate: toFloat($absorption_rate),
            rate_per_unit: toFloat($rate_per_unit),
            created_at: datetime($created_at)
        })
        CREATE (u)-[:OWNS_ABSORPTION_RATE]->(x)
        RETURN x
        """,
        {
            "id": rate.id,
            "user_id": user_id,
            "cost_centre_name": cost_centre_name,
            "cost_centre_id": cost_centre_id,
            "overhead_type": overhead_type,
            "absorption_base": absorption_base,
            "budgeted_overhead": budgeted_overhead,
            "budgeted_base_units": budgeted_base_units,
            "absorption_rate": rate.absorption_rate,
            "rate_per_unit": rate.rate_per_unit,
            "created_at": rate.created_at.isoformat(),
        },
    )
    return rate


async def list_rates(
    session: AsyncSession, user_id: str, cost_centre_id: Optional[str] = None
) -> List[OverheadAbsorptionRate]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ABSORPTION_RATE]->(x:OverheadAbsorptionRate)
    {RATE_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    rates = [_rate_from_node(dict(r["x"]), user_id) async for r in result]
    if cost_centre_id:
        rates = [r for r in rates if r.cost_centre_id == cost_centre_id]
    return rates


async def record_analysis(
    session: AsyncSession,
    user_id: str,
    *,
    entity_name: str,
    cost_centre_name: str,
    rate_type: str,
    absorption_base: str,
    calculated_rate: float,
    base_units_used: float,
    overhead_absorbed: float,
) -> OARAnalysis:
    analysis = OARAnalysis(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        entity_name=entity_name,
        cost_centre_name=cost_centre_name,
        rate_type=rate_type,
        absorption_base=absorption_base,
        calculated_rate=calculated_rate,
        base_units_used=base_units_used,
        overhead_absorbed=overhead_absorbed,
    )
    await _run(
        session,
        """
        MATCH (u:User {id: $user_id})
        CREATE (x:OARAnalysis {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            entity_name: $entity_name,
            cost_centre_name: $cost_centre_name,
            rate_type: $rate_type,
            absorption_base: $absorption_base,
            calculated_rate: toFloat($calculated_rate),
            base_units_used: toFloat($base_units_used),
            overhead_absorbed: toFloat($overhead_absorbed),
            created_at: datetime($created_at)
        })
        CREATE (u)-[:OWNS_OAR_ANALYSIS]->(x)
        RETURN x
        """,
        {
            "id": analysis.id,
            "user_id": user_id,
            "entity_name": entity_name,
            "cost_centre_name": cost_centre_name,
            "rate_type": rate_type,
            "absorption_base": absorption_base,
            "calculated_rate": calculated_rate,
            "base_units_used": base_units_used,
            "overhead_absorbed": overhead_absorbed,
            "created_at": analysis.created_at.isoformat(),
        },
    )
    return analysis


async def list_analyses(session: AsyncSession, user_id: str) -> List[OARAnalysis]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_OAR_ANALYSIS]->(x:OARAnalysis)
    {RATE_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    return [_analysis_from_node(dict(r["x"]), user_id) async for r in result]
