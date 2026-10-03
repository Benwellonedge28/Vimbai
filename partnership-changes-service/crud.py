"""
Partnership Changes Service CRUD Operations

Partner changes and admission details persist as Neo4j nodes,
caller-owned (X-User-Id) and Book-gated (X-Book-ID). Dict/List fields
(premium_distribution, ratios, journal_entry_ids) are stored as JSON
props. Settlement status is written back via SET assignments.
"""

import json
from datetime import datetime, timezone
from typing import Dict, List, Optional

from neo4j import AsyncSession
from partnership_changes_service.dependencies import book_id_var
from partnership_changes_service.models import AdmissionDetails, ChangeType, PartnerChange

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

_DT_FIELDS = ("effective_date", "admission_date", "created_at")
_FLOAT_FIELDS = (
    "capital_balance",
    "current_account_balance",
    "total_payable",
    "goodwill_amount",
    "capital_contribution",
    "goodwill_paid",
    "revaluation_amount",
)


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


def _change_from_node(n: Dict) -> PartnerChange:
    return PartnerChange(
        id=n["id"],
        partnership_id=n.get("partnership_id", ""),
        change_type=ChangeType(n.get("change_type", "admission")),
        partner_id=n.get("partner_id", ""),
        partner_name=n.get("partner_name", ""),
        effective_date=_as_dt(n.get("effective_date")) or datetime.now(timezone.utc),
        capital_balance=float(n.get("capital_balance", 0.0) or 0.0),
        current_account_balance=float(n.get("current_account_balance", 0.0) or 0.0),
        total_payable=float(n.get("total_payable", 0.0) or 0.0),
        goodwill_amount=float(n.get("goodwill_amount", 0.0) or 0.0),
        payment_method=n.get("payment_method", "cash"),
        settlement_status=n.get("settlement_status", "pending"),
        journal_entry_id=n.get("journal_entry_id"),
        notes=n.get("notes"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


def _admission_from_node(n: Dict) -> AdmissionDetails:
    return AdmissionDetails(
        id=n["id"],
        new_partner_id=n.get("new_partner_id", ""),
        new_partner_name=n.get("new_partner_name", ""),
        capital_contribution=float(n.get("capital_contribution", 0.0) or 0.0),
        goodwill_paid=float(n.get("goodwill_paid", 0.0) or 0.0),
        premium_distribution=json.loads(n.get("premium_distribution") or "{}"),
        new_profit_sharing_ratios=json.loads(n.get("new_profit_sharing_ratios") or "{}"),
        revaluation_required=bool(n.get("revaluation_required", False)),
        revaluation_amount=float(n.get("revaluation_amount", 0.0) or 0.0),
        admission_date=_as_dt(n.get("admission_date")) or datetime.now(timezone.utc),
        journal_entry_ids=json.loads(n.get("journal_entry_ids") or "[]"),
    )


# --- partner changes ---


async def create_change(session: AsyncSession, user_id: str, change: PartnerChange) -> PartnerChange:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:PartnerChange {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        partnership_id: $partnership_id,
        change_type: $change_type,
        partner_id: $partner_id,
        partner_name: $partner_name,
        effective_date: datetime($effective_date),
        capital_balance: toFloat($capital_balance),
        current_account_balance: toFloat($current_account_balance),
        total_payable: toFloat($total_payable),
        goodwill_amount: toFloat($goodwill_amount),
        payment_method: $payment_method,
        settlement_status: $settlement_status,
        journal_entry_id: $journal_entry_id,
        notes: $notes,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_CHANGE]->(x)
    RETURN x
    """
    result = await _run(
        session,
        query,
        {
            "id": change.id,
            "partnership_id": change.partnership_id,
            "change_type": change.change_type.value,
            "partner_id": change.partner_id,
            "partner_name": change.partner_name,
            "effective_date": change.effective_date.isoformat(),
            "capital_balance": change.capital_balance,
            "current_account_balance": change.current_account_balance,
            "total_payable": change.total_payable,
            "goodwill_amount": change.goodwill_amount,
            "payment_method": change.payment_method,
            "settlement_status": change.settlement_status,
            "journal_entry_id": change.journal_entry_id,
            "notes": change.notes,
            "created_at": change.created_at.isoformat(),
        },
        user_id=user_id,
    )
    records = [r async for r in result]
    return _change_from_node(dict(records[0]["x"]))


async def list_changes(session: AsyncSession, user_id: str) -> List[PartnerChange]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CHANGE]->(x:PartnerChange)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_change_from_node(dict(r["x"])) async for r in result]


async def get_change(session: AsyncSession, user_id: str, change_id: str) -> Optional[PartnerChange]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CHANGE]->(x:PartnerChange)
    WHERE x.id = $change_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, change_id=change_id, user_id=user_id)
    record = await result.single()
    return _change_from_node(dict(record["x"])) if record else None


async def settle_change(session: AsyncSession, user_id: str, change_id: str) -> None:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_CHANGE]->(x:PartnerChange)
    WHERE x.id = $change_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.settlement_status = $status
    """
    await _run(session, query, {"status": "settled"}, change_id=change_id, user_id=user_id)


# --- admissions ---


async def create_admission(session: AsyncSession, user_id: str, adm: AdmissionDetails) -> AdmissionDetails:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:AdmissionDetails {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        new_partner_id: $new_partner_id,
        new_partner_name: $new_partner_name,
        capital_contribution: toFloat($capital_contribution),
        goodwill_paid: toFloat($goodwill_paid),
        premium_distribution: $premium_distribution,
        new_profit_sharing_ratios: $new_profit_sharing_ratios,
        revaluation_required: $revaluation_required,
        revaluation_amount: toFloat($revaluation_amount),
        admission_date: datetime($admission_date),
        journal_entry_ids: $journal_entry_ids
    })
    CREATE (u)-[:OWNS_ADMISSION]->(x)
    RETURN x
    """
    result = await _run(
        session,
        query,
        {
            "id": adm.id,
            "new_partner_id": adm.new_partner_id,
            "new_partner_name": adm.new_partner_name,
            "capital_contribution": adm.capital_contribution,
            "goodwill_paid": adm.goodwill_paid,
            "premium_distribution": json.dumps(adm.premium_distribution),
            "new_profit_sharing_ratios": json.dumps(adm.new_profit_sharing_ratios),
            "revaluation_required": adm.revaluation_required,
            "revaluation_amount": adm.revaluation_amount,
            "admission_date": adm.admission_date.isoformat(),
            "journal_entry_ids": json.dumps(adm.journal_entry_ids),
        },
        user_id=user_id,
    )
    records = [r async for r in result]
    return _admission_from_node(dict(records[0]["x"]))


async def list_admissions(session: AsyncSession, user_id: str) -> List[AdmissionDetails]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ADMISSION]->(x:AdmissionDetails)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_admission_from_node(dict(r["x"])) async for r in result]
