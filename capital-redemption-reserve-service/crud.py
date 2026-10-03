"""
Capital Redemption Reserve Service CRUD Operations

Redemption transactions, CRR creations, and CRR utilizations persist as
Neo4j nodes, caller-owned (X-User-Id) and Book-gated (X-Book-ID). Each
record carries company_id as a plain prop; all queries are scoped to the
caller's own Book-visible records.
"""

from datetime import datetime, timezone
from typing import Dict, List, Optional

from capital_redemption_reserve_service.dependencies import book_id_var
from capital_redemption_reserve_service.models import CRRCreation, CRRUtilization, RedemptionTransaction
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


def _create_query(label: str, edge: str, props: str) -> str:
    return f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:{label} {{
{props}
    }})
    CREATE (u)-[:{edge}]->(x)
    RETURN x
    """


async def _create(session: AsyncSession, user_id: str, label: str, edge: str, props: str, params: Dict) -> Dict:
    result = await _run(session, _create_query(label, edge, props), params, user_id=user_id)
    records = [rec async for rec in result]
    return dict(records[0]["x"])


def _tx_from_node(n: Dict) -> RedemptionTransaction:
    return RedemptionTransaction(
        id=n["id"],
        company_id=n.get("company_id", ""),
        share_class=n.get("share_class", ""),
        shares_redeemed=int(n.get("shares_redeemed", 0) or 0),
        redemption_price=_f(n.get("redemption_price")),
        nominal_value=_f(n.get("nominal_value")),
        total_proceeds=_f(n.get("total_proceeds")),
        redemption_reserve_amount=_f(n.get("redemption_reserve_amount")),
        redemption_date=_as_dt(n.get("redemption_date")) or datetime.now(timezone.utc),
        source_account=n.get("source_account", ""),
        journal_entry_id=n.get("journal_entry_id"),
        status=n.get("status", "completed"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


async def create_redemption(session: AsyncSession, user_id: str, t: RedemptionTransaction) -> RedemptionTransaction:
    props = """        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        share_class: $share_class,
        shares_redeemed: toInteger($shares_redeemed),
        redemption_price: toFloat($redemption_price),
        nominal_value: toFloat($nominal_value),
        total_proceeds: toFloat($total_proceeds),
        redemption_reserve_amount: toFloat($redemption_reserve_amount),
        redemption_date: datetime($redemption_date),
        source_account: $source_account,
        journal_entry_id: $journal_entry_id,
        status: $status,
        created_at: datetime($created_at)"""
    params = {
        "id": t.id,
        "company_id": t.company_id,
        "share_class": t.share_class,
        "shares_redeemed": t.shares_redeemed,
        "redemption_price": t.redemption_price,
        "nominal_value": t.nominal_value,
        "total_proceeds": t.total_proceeds,
        "redemption_reserve_amount": t.redemption_reserve_amount,
        "redemption_date": _iso(t.redemption_date),
        "source_account": t.source_account,
        "journal_entry_id": t.journal_entry_id,
        "status": t.status,
        "created_at": _iso(t.created_at),
    }
    n = await _create(session, user_id, "RedemptionTransaction", "OWNS_REDEMPTION", props, params)
    return _tx_from_node(n)


async def list_redemptions(session: AsyncSession, user_id: str) -> List[RedemptionTransaction]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_REDEMPTION]->(x:RedemptionTransaction)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_tx_from_node(dict(r["x"])) async for r in result]


def _creation_from_node(n: Dict) -> CRRCreation:
    return CRRCreation(
        id=n["id"],
        company_id=n.get("company_id", ""),
        amount=_f(n.get("amount")),
        source=n.get("source", ""),
        description=n.get("description", ""),
        creation_date=_as_dt(n.get("creation_date")) or datetime.now(timezone.utc),
        journal_entry_id=n.get("journal_entry_id"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


async def create_creation(session: AsyncSession, user_id: str, c: CRRCreation) -> CRRCreation:
    props = """        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        amount: toFloat($amount),
        source: $source,
        description: $description,
        creation_date: datetime($creation_date),
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at)"""
    params = {
        "id": c.id,
        "company_id": c.company_id,
        "amount": c.amount,
        "source": c.source,
        "description": c.description,
        "creation_date": _iso(c.creation_date),
        "journal_entry_id": c.journal_entry_id,
        "created_at": _iso(c.created_at),
    }
    n = await _create(session, user_id, "CRRCreation", "OWNS_CRR_CREATION", props, params)
    return _creation_from_node(n)


async def list_creations(session: AsyncSession, user_id: str) -> List[CRRCreation]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CRR_CREATION]->(x:CRRCreation)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_creation_from_node(dict(r["x"])) async for r in result]


def _util_from_node(n: Dict) -> CRRUtilization:
    return CRRUtilization(
        id=n["id"],
        company_id=n.get("company_id", ""),
        amount=_f(n.get("amount")),
        utilization_type=n.get("utilization_type", ""),
        description=n.get("description", ""),
        utilization_date=_as_dt(n.get("utilization_date")) or datetime.now(timezone.utc),
        journal_entry_id=n.get("journal_entry_id"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


async def create_utilization(session: AsyncSession, user_id: str, u: CRRUtilization) -> CRRUtilization:
    props = """        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        amount: toFloat($amount),
        utilization_type: $utilization_type,
        description: $description,
        utilization_date: datetime($utilization_date),
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at)"""
    params = {
        "id": u.id,
        "company_id": u.company_id,
        "amount": u.amount,
        "utilization_type": u.utilization_type,
        "description": u.description,
        "utilization_date": _iso(u.utilization_date),
        "journal_entry_id": u.journal_entry_id,
        "created_at": _iso(u.created_at),
    }
    n = await _create(session, user_id, "CRRUtilization", "OWNS_CRR_UTILIZATION", props, params)
    return _util_from_node(n)


async def list_utilizations(session: AsyncSession, user_id: str) -> List[CRRUtilization]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CRR_UTILIZATION]->(x:CRRUtilization)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_util_from_node(dict(r["x"])) async for r in result]
