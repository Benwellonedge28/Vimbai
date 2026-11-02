"""
Bad Debts Provision Service CRUD Operations

Three module-level stores become nodes: :ProvisionPolicy via :OWNS_PROVISION_POLICY,
:DebtorRecord via :OWNS_DEBTOR and :ProvisionJournal via :OWNS_PROVISION_JOURNAL.
Every read is Book-gated. The ageing pass persists the days, status and provision
it worked out for each debtor, where the in-memory version lost them on restart.

PYTHON QUIRKS preserved:
* ageing counts from *today* rather than the invoice date, so a period end in the
  past subtracts days before the debtor's own figure is added, and the result is
  clamped at zero;
* a debtor past 365 days is provided at 100% whatever the policy says;
* the under-30 bucket is always 0%, even when the rules give "0" a rate;
* /journal/create never passes a policy, so it always ages against the defaults.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from provision_bad_debts_service.dependencies import book_id_var
from provision_bad_debts_service.exceptions import NotFoundError
from provision_bad_debts_service.models import (
    DebtorStatus,
    ProvisionEntry,
    ProvisionJournal,
    ProvisionMethod,
    ProvisionPolicy,
)

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

# the ladder the service falls back on when no policy is given
DEFAULT_RULES: Dict[str, float] = {"0": 0, "30": 1, "60": 5, "90": 10, "180": 50, "365": 100}


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


def _iso(value: Optional[datetime]) -> Optional[str]:
    return value.isoformat() if value else None


def _entry_from_node(n: Dict[str, Any], user_id: str) -> ProvisionEntry:
    return ProvisionEntry(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        debtor_id=n.get("debtor_id") or "",
        debtor_name=n.get("debtor_name") or "",
        invoice_number=n.get("invoice_number") or "",
        original_amount=float(n.get("original_amount") or 0),
        outstanding_amount=float(n.get("outstanding_amount") or 0),
        days_outstanding=int(n.get("days_outstanding") or 0),
        status=DebtorStatus(n.get("status") or DebtorStatus.CURRENT.value),
        provision_percent=float(n.get("provision_percent") or 0),
        provision_amount=float(n.get("provision_amount") or 0),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _policy_from_node(n: Dict[str, Any], user_id: str) -> ProvisionPolicy:
    return ProvisionPolicy(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        name=n.get("name") or "",
        method=ProvisionMethod(n.get("method") or ProvisionMethod.AGEING_ANALYSIS.value),
        policy_rules=json.loads(n.get("rules_json") or "{}"),
        default_provision_percent=float(n.get("default_provision_percent") or 0),
        is_active=bool(n.get("is_active")),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _journal_from_node(n: Dict[str, Any], user_id: str) -> ProvisionJournal:
    return ProvisionJournal(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        period_end=_as_dt(n.get("period_end")) or _now(),
        total_debtors=float(n.get("total_debtors") or 0),
        provision_opening=float(n.get("provision_opening") or 0),
        provision_required=float(n.get("provision_required") or 0),
        provision_adjustment=float(n.get("provision_adjustment") or 0),
        provision_closing=float(n.get("provision_closing") or 0),
        entries=[ProvisionEntry(**e) for e in json.loads(n.get("entries_json") or "[]")],
        journal_entry_id=n.get("journal_entry_id"),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


async def create_policy(session: AsyncSession, user_id: str, policy: ProvisionPolicy) -> ProvisionPolicy:
    policy.id = str(uuid.uuid4())
    policy.user_id = user_id
    policy.book_id = book_id_var.get()
    policy.created_at = _now()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:ProvisionPolicy {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        method: $method,
        rules_json: $rules_json,
        default_provision_percent: toFloat($default_provision_percent),
        is_active: $is_active,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_PROVISION_POLICY]->(x)
    RETURN x
    """
    await _run(
        session,
        query,
        {
            "id": policy.id,
            "user_id": user_id,
            "name": policy.name,
            "method": policy.method.value,
            "rules_json": json.dumps(policy.policy_rules),
            "default_provision_percent": policy.default_provision_percent,
            "is_active": policy.is_active,
            "created_at": policy.created_at.isoformat(),
        },
    )
    return policy


async def list_policies(session: AsyncSession, user_id: str, is_active: Optional[bool] = None) -> List[ProvisionPolicy]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PROVISION_POLICY]->(x:ProvisionPolicy)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    policies = [_policy_from_node(dict(r["x"]), user_id) async for r in result]
    if is_active is not None:
        policies = [p for p in policies if p.is_active == is_active]
    return policies


async def get_policy(session: AsyncSession, user_id: str, policy_id: str) -> Optional[ProvisionPolicy]:
    if not policy_id:
        return None
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PROVISION_POLICY]->(x:ProvisionPolicy {{id: $policy_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, policy_id=policy_id)
    records = [r async for r in result]
    if not records:
        return None
    return _policy_from_node(dict(records[0]["x"]), user_id)


async def add_debtor(
    session: AsyncSession,
    user_id: str,
    *,
    debtor_id: str,
    debtor_name: str,
    invoice_number: str,
    original_amount: float,
) -> ProvisionEntry:
    entry = ProvisionEntry(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        debtor_id=debtor_id,
        debtor_name=debtor_name,
        invoice_number=invoice_number,
        original_amount=original_amount,
        outstanding_amount=original_amount,
        days_outstanding=0,
        status=DebtorStatus.CURRENT,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:DebtorRecord {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        debtor_id: $debtor_id,
        debtor_name: $debtor_name,
        invoice_number: $invoice_number,
        original_amount: toFloat($original_amount),
        outstanding_amount: toFloat($outstanding_amount),
        days_outstanding: 0,
        status: $status,
        provision_percent: toFloat(0),
        provision_amount: toFloat(0),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_DEBTOR]->(x)
    RETURN x
    """
    await _run(
        session,
        query,
        {
            "id": entry.id,
            "user_id": user_id,
            "debtor_id": debtor_id,
            "debtor_name": debtor_name,
            "invoice_number": invoice_number,
            "original_amount": original_amount,
            "outstanding_amount": entry.outstanding_amount,
            "status": entry.status.value,
            "created_at": _now().isoformat(),
        },
    )
    return entry


async def list_debtors(session: AsyncSession, user_id: str) -> List[ProvisionEntry]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DEBTOR]->(x:DebtorRecord)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    return [_entry_from_node(dict(r["x"]), user_id) async for r in result]


def _bucket(days: int, rules: Dict[str, float]) -> tuple[DebtorStatus, float]:
    """Age a debtor into a status and a rate, exactly as the service does."""
    if days >= 365:
        return DebtorStatus.BAD, 100.0
    if days >= 180:
        return DebtorStatus.OVERDUE_180, float(rules.get("180", 50))
    if days >= 90:
        return DebtorStatus.OVERDUE_90, float(rules.get("90", 10))
    if days >= 60:
        return DebtorStatus.OVERDUE_60, float(rules.get("60", 5))
    if days >= 30:
        return DebtorStatus.OVERDUE_30, float(rules.get("30", 1))
    return DebtorStatus.CURRENT, 0.0


async def calculate_provision(
    session: AsyncSession,
    user_id: str,
    period_end: datetime,
    policy_id: Optional[str] = None,
    total_debtors: float = 0,
) -> Dict[str, Any]:
    policy = await get_policy(session, user_id, policy_id or "")
    rules = dict(policy.policy_rules) if policy else dict(DEFAULT_RULES)

    debtors = await list_debtors(session, user_id)
    provision_required = 0.0
    entries: List[ProvisionEntry] = []
    # ageing is done in naive UTC, as the original's utcnow() arithmetic was
    now = datetime.now(timezone.utc).replace(tzinfo=None)
    period = _as_dt(period_end) or datetime.now(timezone.utc)
    period = period.replace(tzinfo=None)

    for debtor in debtors:
        days = (period - now).days + debtor.days_outstanding
        debtor.days_outstanding = max(0, days)
        debtor.status, debtor.provision_percent = _bucket(debtor.days_outstanding, rules)
        debtor.provision_amount = debtor.outstanding_amount * (debtor.provision_percent / 100)
        provision_required += debtor.provision_amount
        entries.append(debtor)
        # note: the invoice date is never consulted; this ages from today

        await _run(
            session,
            """
            MATCH (x:DebtorRecord {id: $id})
            SET x.days_outstanding = $days,
                x.status = $status,
                x.provision_percent = toFloat($percent),
                x.provision_amount = toFloat($amount)
            """,
            {
                "id": debtor.id,
                "days": debtor.days_outstanding,
                "status": debtor.status.value,
                "percent": debtor.provision_percent,
                "amount": debtor.provision_amount,
            },
        )

    return {
        "total_debtors": total_debtors or sum(d.outstanding_amount for d in entries),
        "provision_required": provision_required,
        "entries": entries,
        "ageing_summary": {
            "current": sum(e.outstanding_amount for e in entries if e.status == DebtorStatus.CURRENT),
            "overdue_30": sum(e.outstanding_amount for e in entries if e.status == DebtorStatus.OVERDUE_30),
            "overdue_60": sum(e.outstanding_amount for e in entries if e.status == DebtorStatus.OVERDUE_60),
            "overdue_90": sum(e.outstanding_amount for e in entries if e.status == DebtorStatus.OVERDUE_90),
            "overdue_180": sum(e.outstanding_amount for e in entries if e.status == DebtorStatus.OVERDUE_180),
            "bad": sum(e.outstanding_amount for e in entries if e.status == DebtorStatus.BAD),
        },
    }


async def create_journal(
    session: AsyncSession,
    user_id: str,
    period_end: datetime,
    total_debtors: float,
    provision_opening: float = 0,
    provision_required: Optional[float] = None,
    entries: Optional[List[ProvisionEntry]] = None,
    journal_entry_id: Optional[str] = None,
) -> ProvisionJournal:
    adjustment = (provision_required or 0) - provision_opening
    journal = ProvisionJournal(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        period_end=period_end,
        total_debtors=total_debtors,
        provision_opening=provision_opening,
        provision_required=provision_required or 0,
        provision_adjustment=adjustment,
        provision_closing=provision_required or 0,
        entries=entries or [],
        journal_entry_id=journal_entry_id,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:ProvisionJournal {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        period_end: datetime($period_end),
        total_debtors: toFloat($total_debtors),
        provision_opening: toFloat($opening),
        provision_required: toFloat($required),
        provision_adjustment: toFloat($adjustment),
        provision_closing: toFloat($closing),
        entries_json: $entries_json,
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_PROVISION_JOURNAL]->(x)
    RETURN x
    """
    await _run(
        session,
        query,
        {
            "id": journal.id,
            "user_id": user_id,
            "period_end": _iso(period_end),
            "total_debtors": journal.total_debtors,
            "opening": journal.provision_opening,
            "required": journal.provision_required,
            "adjustment": journal.provision_adjustment,
            "closing": journal.provision_closing,
            "entries_json": json.dumps([e.model_dump(mode="json") for e in journal.entries]),
            "journal_entry_id": journal_entry_id,
            "created_at": _now().isoformat(),
        },
    )
    return journal


async def list_journals(session: AsyncSession, user_id: str, limit: int = 12) -> List[ProvisionJournal]:
    """The history is the tail of the journals, oldest first inside that tail."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PROVISION_JOURNAL]->(x:ProvisionJournal)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    journals = [_journal_from_node(dict(r["x"]), user_id) async for r in result]
    if limit and limit > 0:
        journals = journals[-limit:]
    return journals
