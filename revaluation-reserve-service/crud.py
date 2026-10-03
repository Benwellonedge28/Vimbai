"""
Revaluation Reserve Service CRUD Operations

Revaluation entries, utilizations, and cumulative revaluations persist
as Neo4j nodes, caller-owned (X-User-Id) and Book-gated (X-Book-ID).
Cumulative nodes are per caller+asset (upsert), matching the original
get-or-create semantics but scoped.
"""

from datetime import datetime, timezone
from typing import Dict, List, Optional

from neo4j import AsyncSession
from revaluation_reserve_service.dependencies import book_id_var
from revaluation_reserve_service.models import CumulativeRevaluation, RevaluationEntry, RevaluationUtilization

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session: AsyncSession, query: str, params: Optional[Dict] = None, **kw):
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


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


def _iso(dt: Optional[datetime]) -> Optional[str]:
    return dt.isoformat() if dt else None


def _f(v, default: float = 0.0) -> float:
    return float(v) if v is not None else default


# --- revaluation entries ---


def _entry_from_node(n: Dict) -> RevaluationEntry:
    return RevaluationEntry(
        id=n["id"],
        company_id=n.get("company_id", ""),
        asset_id=n.get("asset_id", ""),
        asset_name=n.get("asset_name", ""),
        asset_class=n.get("asset_class", ""),
        revaluation_date=_as_dt(n.get("revaluation_date")) or datetime.now(timezone.utc),
        previous_value=_f(n.get("previous_value")),
        new_value=_f(n.get("new_value")),
        revaluation_gain=_f(n.get("revaluation_gain")),
        revaluation_loss=_f(n.get("revaluation_loss")),
        depreciation_adjustment=_f(n.get("depreciation_adjustment")),
        net_effect=_f(n.get("net_effect")),
        journal_entry_id=n.get("journal_entry_id"),
        status=n.get("status", "completed"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


async def create_entry(session: AsyncSession, user_id: str, e: RevaluationEntry) -> RevaluationEntry:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:RevaluationEntry {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        asset_id: $asset_id,
        asset_name: $asset_name,
        asset_class: $asset_class,
        revaluation_date: datetime($revaluation_date),
        previous_value: toFloat($previous_value),
        new_value: toFloat($new_value),
        revaluation_gain: toFloat($revaluation_gain),
        revaluation_loss: toFloat($revaluation_loss),
        depreciation_adjustment: toFloat($depreciation_adjustment),
        net_effect: toFloat($net_effect),
        journal_entry_id: $journal_entry_id,
        status: $status,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_REVALUATION_ENTRY]->(x)
    RETURN x
    """
    params = {
        "id": e.id,
        "company_id": e.company_id,
        "asset_id": e.asset_id,
        "asset_name": e.asset_name,
        "asset_class": e.asset_class,
        "revaluation_date": _iso(e.revaluation_date),
        "previous_value": e.previous_value,
        "new_value": e.new_value,
        "revaluation_gain": e.revaluation_gain,
        "revaluation_loss": e.revaluation_loss,
        "depreciation_adjustment": e.depreciation_adjustment,
        "net_effect": e.net_effect,
        "journal_entry_id": e.journal_entry_id,
        "status": e.status,
        "created_at": _iso(e.created_at),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _entry_from_node(dict(records[0]["x"]))


async def list_entries(session: AsyncSession, user_id: str) -> List[RevaluationEntry]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_REVALUATION_ENTRY]->(x:RevaluationEntry)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_entry_from_node(dict(r["x"])) async for r in result]


# --- utilizations ---


def _util_from_node(n: Dict) -> RevaluationUtilization:
    return RevaluationUtilization(
        id=n["id"],
        company_id=n.get("company_id", ""),
        amount=_f(n.get("amount")),
        utilization_type=n.get("utilization_type", ""),
        related_asset_id=n.get("related_asset_id"),
        description=n.get("description", ""),
        journal_entry_id=n.get("journal_entry_id"),
        utilization_date=_as_dt(n.get("utilization_date")) or datetime.now(timezone.utc),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


async def create_utilization(session: AsyncSession, user_id: str, u: RevaluationUtilization) -> RevaluationUtilization:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:RevaluationUtilization {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        amount: toFloat($amount),
        utilization_type: $utilization_type,
        related_asset_id: $related_asset_id,
        description: $description,
        journal_entry_id: $journal_entry_id,
        utilization_date: datetime($utilization_date),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_REVALUATION_UTILIZATION]->(x)
    RETURN x
    """
    params = {
        "id": u.id,
        "company_id": u.company_id,
        "amount": u.amount,
        "utilization_type": u.utilization_type,
        "related_asset_id": u.related_asset_id,
        "description": u.description,
        "journal_entry_id": u.journal_entry_id,
        "utilization_date": _iso(u.utilization_date),
        "created_at": _iso(u.created_at),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _util_from_node(dict(records[0]["x"]))


async def list_utilizations(session: AsyncSession, user_id: str) -> List[RevaluationUtilization]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_REVALUATION_UTILIZATION]->(x:RevaluationUtilization)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_util_from_node(dict(r["x"])) async for r in result]


# --- cumulative revaluations (per caller+asset) ---


def _cum_from_node(n: Dict) -> CumulativeRevaluation:
    return CumulativeRevaluation(
        id=n["id"],
        company_id=n.get("company_id", ""),
        asset_id=n.get("asset_id", ""),
        total_revaluation_gain=_f(n.get("total_revaluation_gain")),
        total_revaluation_loss=_f(n.get("total_revaluation_loss")),
        total_utilized=_f(n.get("total_utilized")),
        net_revaluation_reserve=_f(n.get("net_revaluation_reserve")),
        last_revaluation_date=_as_dt(n.get("last_revaluation_date")),
    )


def _cum_params(c: CumulativeRevaluation) -> Dict:
    return {
        "id": c.id,
        "company_id": c.company_id,
        "asset_id": c.asset_id,
        "total_revaluation_gain": c.total_revaluation_gain,
        "total_revaluation_loss": c.total_revaluation_loss,
        "total_utilized": c.total_utilized,
        "net_revaluation_reserve": c.net_revaluation_reserve,
        "last_revaluation_date": _iso(c.last_revaluation_date),
    }


_CUM_SET = """x.company_id = $company_id,
        x.total_revaluation_gain = toFloat($total_revaluation_gain),
        x.total_revaluation_loss = toFloat($total_revaluation_loss),
        x.total_utilized = toFloat($total_utilized),
        x.net_revaluation_reserve = toFloat($net_revaluation_reserve),
        x.last_revaluation_date = datetime($last_revaluation_date)"""


async def get_cumulative(session: AsyncSession, user_id: str, asset_id: str) -> Optional[CumulativeRevaluation]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CUMULATIVE_REVALUATION]->(x:CumulativeRevaluation)
    WHERE x.asset_id = $asset_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, asset_id=asset_id, user_id=user_id)
    record = await result.single()
    return _cum_from_node(dict(record["x"])) if record else None


async def save_cumulative(session: AsyncSession, user_id: str, c: CumulativeRevaluation) -> CumulativeRevaluation:
    """Upsert the caller's cumulative node for the asset."""
    existing = await get_cumulative(session, user_id, c.asset_id)
    if existing:
        query = f"""
        MATCH (u:User {{id: $user_id}})-[:OWNS_CUMULATIVE_REVALUATION]->(x:CumulativeRevaluation)
        WHERE x.asset_id = $asset_id AND ($book_id IS NULL OR x.book_id = $book_id)
        SET {_CUM_SET}
        RETURN x
        """
        result = await _run(session, query, _cum_params(c), asset_id=c.asset_id, user_id=user_id)
        record = await result.single()
        return _cum_from_node(dict(record["x"]))

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:CumulativeRevaluation {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        asset_id: $asset_id,
        total_revaluation_gain: toFloat($total_revaluation_gain),
        total_revaluation_loss: toFloat($total_revaluation_loss),
        total_utilized: toFloat($total_utilized),
        net_revaluation_reserve: toFloat($net_revaluation_reserve),
        last_revaluation_date: datetime($last_revaluation_date)
    })
    CREATE (u)-[:OWNS_CUMULATIVE_REVALUATION]->(x)
    RETURN x
    """
    result = await _run(session, query, _cum_params(c), user_id=user_id)
    record = await result.single()
    return _cum_from_node(dict(record["x"]))
