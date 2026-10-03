"""
Debentures Service CRUD Operations

Debenture classes, issues, interest payments, and redemptions persist as
Neo4j nodes, caller-owned (X-User-Id) and Book-gated (X-Book-ID). Class
counters and interest status are Python-computed and written back to the
nodes via params (the fake test harness cannot do node-prop arithmetic in
SET). Not-found lookups return None; the caller keeps the original
{"error": ...} 200 response shape.
"""

from datetime import datetime, timezone
from typing import Dict, List, Optional

from debentures_service.dependencies import book_id_var
from debentures_service.models import (
    DebentureClass,
    DebentureIssue,
    InterestPayment,
    RedemptionEntry,
)
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

_DT_FIELDS = (
    "issue_date",
    "period_start",
    "period_end",
    "payment_date",
    "redemption_date",
    "maturity_date",
    "created_at",
)
_FLOAT_FIELDS = (
    "nominal_value",
    "issue_price",
    "coupon_rate",
    "redemption_price",
    "total_proceeds",
    "discount_on_issue",
    "premium_on_redemption",
    "interest_rate",
    "interest_amount",
    "tax_deducted",
    "net_payment",
)
_INT_FIELDS = ("debentures_issued", "debentures_outstanding", "debentures_redeemed")


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


def _hydrate(model_cls, n: Dict):
    kwargs = {}
    for field in (
        "id",
        "name",
        "company_id",
        "debenture_class_id",
        "interest_payment_frequency",
        "convertibility",
        "conversion_terms",
        "status",
        "journal_entry_id",
    ):
        if field in n:
            kwargs[field] = n[field]
    for field in _DT_FIELDS:
        if field in n:
            kwargs[field] = _as_dt(n[field])
    for field in _FLOAT_FIELDS:
        if field in n:
            kwargs[field] = float(n[field])
    for field in _INT_FIELDS:
        if field in n:
            kwargs[field] = int(n[field])
    return model_cls(**kwargs)


def _create_query(label: str, edge: str, fields: List[str]) -> str:
    props = []
    for f in fields:
        if f in _DT_FIELDS:
            props.append(f"{f}: datetime(${f})")
        elif f in _FLOAT_FIELDS:
            props.append(f"{f}: toFloat(${f})")
        elif f in _INT_FIELDS:
            props.append(f"{f}: toInteger(${f})")
        else:
            props.append(f"{f}: ${f}")
    return f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:{label} {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        {', '.join(props)}
    }})
    CREATE (u)-[:{edge}]->(x)
    RETURN x
    """


async def _create(session: AsyncSession, user_id: str, label: str, edge: str, model, fields: List[str]):
    params = {"id": model.id}
    for f in fields:
        v = getattr(model, f)
        if isinstance(v, datetime):
            v = v.isoformat()
        params[f] = v
    result = await _run(session, _create_query(label, edge, fields), params, user_id=user_id)
    records = [r async for r in result]
    return _hydrate(type(model), dict(records[0]["x"]))


async def _list(session: AsyncSession, user_id: str, label: str, edge: str, model_cls) -> List:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_hydrate(model_cls, dict(r["x"])) async for r in result]


async def _get_by_id(session: AsyncSession, user_id: str, label: str, edge: str, model_cls, node_id: str):
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    WHERE x.id = $node_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, node_id=node_id)
    record = await result.single()
    return _hydrate(model_cls, dict(record["x"])) if record else None


# --- debenture classes ---


async def create_debenture_class(session: AsyncSession, user_id: str, deb_class: DebentureClass) -> DebentureClass:
    fields = [
        "name",
        "company_id",
        "nominal_value",
        "issue_price",
        "coupon_rate",
        "interest_payment_frequency",
        "maturity_date",
        "redemption_price",
        "convertibility",
        "conversion_terms",
        "debentures_issued",
        "debentures_outstanding",
    ]
    return await _create(session, user_id, "DebentureClass", "OWNS_DEBENTURE_CLASS", deb_class, fields)


async def list_debenture_classes(session: AsyncSession, user_id: str) -> List[DebentureClass]:
    return await _list(session, user_id, "DebentureClass", "OWNS_DEBENTURE_CLASS", DebentureClass)


async def get_debenture_class(session: AsyncSession, user_id: str, class_id: str) -> Optional[DebentureClass]:
    return await _get_by_id(session, user_id, "DebentureClass", "OWNS_DEBENTURE_CLASS", DebentureClass, class_id)


async def update_class_counters(
    session: AsyncSession, user_id: str, class_id: str, issued: int, outstanding: int
) -> None:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_DEBENTURE_CLASS]->(x:DebentureClass)
    WHERE x.id = $class_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.debentures_issued = toInteger($issued),
        x.debentures_outstanding = toInteger($outstanding)
    """
    await _run(session, query, {"issued": issued, "outstanding": outstanding}, user_id=user_id, class_id=class_id)


# --- issues ---


async def create_issue(session: AsyncSession, user_id: str, issue: DebentureIssue) -> DebentureIssue:
    fields = [
        "company_id",
        "debenture_class_id",
        "debentures_issued",
        "issue_date",
        "total_proceeds",
        "discount_on_issue",
        "journal_entry_id",
        "created_at",
    ]
    return await _create(session, user_id, "DebentureIssue", "OWNS_ISSUE", issue, fields)


async def list_issues(session: AsyncSession, user_id: str) -> List[DebentureIssue]:
    return await _list(session, user_id, "DebentureIssue", "OWNS_ISSUE", DebentureIssue)


# --- interest ---


async def create_interest(session: AsyncSession, user_id: str, interest: InterestPayment) -> InterestPayment:
    fields = [
        "company_id",
        "debenture_class_id",
        "period_start",
        "period_end",
        "debentures_outstanding",
        "interest_rate",
        "interest_amount",
        "tax_deducted",
        "net_payment",
        "payment_date",
        "journal_entry_id",
        "status",
        "created_at",
    ]
    return await _create(session, user_id, "InterestPayment", "OWNS_INTEREST", interest, fields)


async def list_interest(session: AsyncSession, user_id: str) -> List[InterestPayment]:
    return await _list(session, user_id, "InterestPayment", "OWNS_INTEREST", InterestPayment)


async def get_interest(session: AsyncSession, user_id: str, interest_id: str) -> Optional[InterestPayment]:
    return await _get_by_id(session, user_id, "InterestPayment", "OWNS_INTEREST", InterestPayment, interest_id)


async def update_interest_payment(
    session: AsyncSession, user_id: str, interest_id: str, payment_date, status: str, journal_entry_id
) -> None:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_INTEREST]->(x:InterestPayment)
    WHERE x.id = $interest_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.payment_date = datetime($payment_date),
        x.status = $status,
        x.journal_entry_id = $journal_entry_id
    """
    await _run(
        session,
        query,
        {
            "payment_date": payment_date.isoformat() if payment_date else None,
            "status": status,
            "journal_entry_id": journal_entry_id,
        },
        user_id=user_id,
        interest_id=interest_id,
    )


# --- redemptions ---


async def create_redemption(session: AsyncSession, user_id: str, redemption: RedemptionEntry) -> RedemptionEntry:
    fields = [
        "company_id",
        "debenture_class_id",
        "debentures_redeemed",
        "redemption_date",
        "redemption_price",
        "total_proceeds",
        "premium_on_redemption",
        "journal_entry_id",
        "created_at",
    ]
    return await _create(session, user_id, "RedemptionEntry", "OWNS_REDEMPTION", redemption, fields)


async def update_journal_entry(
    session: AsyncSession, user_id: str, label: str, edge: str, node_id: str, journal_entry_id: Optional[str]
) -> None:
    """Persist a journal-entry id obtained from the accounting side-call."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    WHERE x.id = $node_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.journal_entry_id = $journal_entry_id
    """
    await _run(session, query, {"journal_entry_id": journal_entry_id}, user_id=user_id, node_id=node_id)
