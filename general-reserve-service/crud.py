"""
General Reserve Service CRUD Operations

Reserves, allocations, and utilizations persist as Neo4j nodes,
caller-owned (X-User-Id) and Book-gated (X-Book-ID). Allocate/utilize
operations check the caller's own Book-visible reserve first, then
write the child record and update the parent balance.
"""

from datetime import datetime, timezone
from typing import Dict, List, Optional

from general_reserve_service.dependencies import book_id_var
from general_reserve_service.models import GeneralReserve, ReserveAllocation, ReserveUtilization
from neo4j import AsyncSession

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


# --- reserves ---


def _reserve_from_node(n: Dict) -> GeneralReserve:
    return GeneralReserve(
        id=n["id"],
        company_id=n.get("company_id", ""),
        reserve_name=n.get("reserve_name", ""),
        description=n.get("description", ""),
        current_balance=_f(n.get("current_balance")),
        target_balance=_f(n.get("target_balance"), 0.0) if n.get("target_balance") is not None else None,
        minimum_balance=_f(n.get("minimum_balance")),
        funding_source=n.get("funding_source", "retained_earnings"),
        journal_entry_id=n.get("journal_entry_id"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
        updated_at=_as_dt(n.get("updated_at")) or datetime.now(timezone.utc),
    )


def _reserve_params(r: GeneralReserve) -> Dict:
    return {
        "id": r.id,
        "company_id": r.company_id,
        "reserve_name": r.reserve_name,
        "description": r.description,
        "current_balance": r.current_balance,
        "target_balance": r.target_balance,
        "minimum_balance": r.minimum_balance,
        "funding_source": r.funding_source,
        "journal_entry_id": r.journal_entry_id,
        "created_at": _iso(r.created_at),
        "updated_at": _iso(r.updated_at),
    }


_PROPS = """        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        reserve_name: $reserve_name,
        description: $description,
        current_balance: toFloat($current_balance),
        target_balance: toFloat($target_balance),
        minimum_balance: toFloat($minimum_balance),
        funding_source: $funding_source,
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)"""

_SET = """x.id = $id,
        x.company_id = $company_id,
        x.reserve_name = $reserve_name,
        x.description = $description,
        x.current_balance = toFloat($current_balance),
        x.target_balance = toFloat($target_balance),
        x.minimum_balance = toFloat($minimum_balance),
        x.funding_source = $funding_source,
        x.journal_entry_id = $journal_entry_id,
        x.created_at = datetime($created_at),
        x.updated_at = datetime($updated_at)"""


async def create_reserve(session: AsyncSession, user_id: str, r: GeneralReserve) -> GeneralReserve:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:GeneralReserve {{
{_PROPS}
    }})
    CREATE (u)-[:OWNS_RESERVE]->(x)
    RETURN x
    """
    result = await _run(session, query, _reserve_params(r), user_id=user_id)
    records = [rec async for rec in result]
    return _reserve_from_node(dict(records[0]["x"]))


async def list_reserves(session: AsyncSession, user_id: str) -> List[GeneralReserve]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RESERVE]->(x:GeneralReserve)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_reserve_from_node(dict(r["x"])) async for r in result]


async def get_reserve(session: AsyncSession, user_id: str, reserve_id: str) -> Optional[GeneralReserve]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RESERVE]->(x:GeneralReserve)
    WHERE x.id = $reserve_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, reserve_id=reserve_id, user_id=user_id)
    record = await result.single()
    return _reserve_from_node(dict(record["x"])) if record else None


async def save_reserve(session: AsyncSession, user_id: str, r: GeneralReserve) -> None:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RESERVE]->(x:GeneralReserve)
    WHERE x.id = $id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET {_SET}
    """
    await _run(session, query, _reserve_params(r), user_id=user_id)


# --- allocations ---


def _alloc_from_node(n: Dict) -> ReserveAllocation:
    return ReserveAllocation(
        id=n["id"],
        reserve_id=n.get("reserve_id", ""),
        amount=_f(n.get("amount")),
        allocation_date=_as_dt(n.get("allocation_date")) or datetime.now(timezone.utc),
        source=n.get("source", ""),
        description=n.get("description", ""),
        journal_entry_id=n.get("journal_entry_id"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


async def create_allocation(session: AsyncSession, user_id: str, a: ReserveAllocation) -> ReserveAllocation:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:ReserveAllocation {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        reserve_id: $reserve_id,
        amount: toFloat($amount),
        allocation_date: datetime($allocation_date),
        source: $source,
        description: $description,
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_ALLOCATION]->(x)
    RETURN x
    """
    params = {
        "id": a.id,
        "reserve_id": a.reserve_id,
        "amount": a.amount,
        "allocation_date": _iso(a.allocation_date),
        "source": a.source,
        "description": a.description,
        "journal_entry_id": a.journal_entry_id,
        "created_at": _iso(a.created_at),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _alloc_from_node(dict(records[0]["x"]))


async def list_allocations(session: AsyncSession, user_id: str, reserve_id: str) -> List[ReserveAllocation]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ALLOCATION]->(x:ReserveAllocation)
    WHERE x.reserve_id = $reserve_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, reserve_id=reserve_id, user_id=user_id)
    return [_alloc_from_node(dict(r["x"])) async for r in result]


# --- utilizations ---


def _util_from_node(n: Dict) -> ReserveUtilization:
    return ReserveUtilization(
        id=n["id"],
        reserve_id=n.get("reserve_id", ""),
        amount=_f(n.get("amount")),
        utilization_date=_as_dt(n.get("utilization_date")) or datetime.now(timezone.utc),
        purpose=n.get("purpose", ""),
        description=n.get("description", ""),
        journal_entry_id=n.get("journal_entry_id"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


async def create_utilization(session: AsyncSession, user_id: str, u: ReserveUtilization) -> ReserveUtilization:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:ReserveUtilization {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        reserve_id: $reserve_id,
        amount: toFloat($amount),
        utilization_date: datetime($utilization_date),
        purpose: $purpose,
        description: $description,
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_UTILIZATION]->(x)
    RETURN x
    """
    params = {
        "id": u.id,
        "reserve_id": u.reserve_id,
        "amount": u.amount,
        "utilization_date": _iso(u.utilization_date),
        "purpose": u.purpose,
        "description": u.description,
        "journal_entry_id": u.journal_entry_id,
        "created_at": _iso(u.created_at),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _util_from_node(dict(records[0]["x"]))


async def list_utilizations(session: AsyncSession, user_id: str, reserve_id: str) -> List[ReserveUtilization]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_UTILIZATION]->(x:ReserveUtilization)
    WHERE x.reserve_id = $reserve_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, reserve_id=reserve_id, user_id=user_id)
    return [_util_from_node(dict(r["x"])) async for r in result]
