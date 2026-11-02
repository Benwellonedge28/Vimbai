"""
Doubtful Debts Provision Service CRUD Operations

:DebtorRecord via :OWNS_DOUBTFUL_DEBTOR and :DoubtfulDebtProvision via
:OWNS_DOUBTFUL_PROVISION, both Book-gated. The risk ladder is code-defined, since
it is a policy constant rather than a record anybody edits.

PYTHON QUIRKS preserved:
* a debtor is looked up by debtor_id and the first match wins, so re-adding the
  same debtor_id leaves the older record in charge of that id;
* ageing escalates only: under 60 days overdue whatever grade was set stays, and
  nothing is ever downgraded as a debt ages back inside a bucket;
* the escalation ignores the configured ladder beyond its fixed rates (60 days is
  medium at 5%, 90 days high at 25%, 180 days doubtful at 75%);
* an unrecognised risk grade is provided at 5%;
* total_debtors counts debtors here, not money.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from provision_doubtful_debts_service.dependencies import book_id_var
from provision_doubtful_debts_service.exceptions import NotFoundError
from provision_doubtful_debts_service.models import (
    DoubtfulDebtEntry,
    DoubtfulDebtProvision,
    ProvisionType,
)

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

# the service's own ladder; an unlisted grade falls back to 5%
DEFAULT_RISK_POLICIES: Dict[str, float] = {"low": 1, "medium": 5, "high": 25, "doubtful": 75}


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


def _entry_from_node(n: Dict[str, Any], user_id: str) -> DoubtfulDebtEntry:
    return DoubtfulDebtEntry(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        debtor_id=n.get("debtor_id") or "",
        debtor_name=n.get("debtor_name") or "",
        invoice_number=n.get("invoice_number") or "",
        invoice_date=_as_dt(n.get("invoice_date")) or _now(),
        due_date=_as_dt(n.get("due_date")) or _now(),
        amount=float(n.get("amount") or 0),
        outstanding=float(n.get("outstanding") or 0),
        days_overdue=int(n.get("days_overdue") or 0),
        risk_assessment=n.get("risk_assessment") or "medium",
        provision_percent=float(n.get("provision_percent") or 0),
        provision_amount=float(n.get("provision_amount") or 0),
        notes=n.get("notes"),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _provision_from_node(n: Dict[str, Any], user_id: str) -> DoubtfulDebtProvision:
    return DoubtfulDebtProvision(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        period_end=_as_dt(n.get("period_end")) or _now(),
        provision_type=ProvisionType(n.get("provision_type") or ProvisionType.SPECIFIC.value),
        total_debtors=float(n.get("total_debtors") or 0),
        provision_required=float(n.get("provision_required") or 0),
        provision_opening=float(n.get("provision_opening") or 0),
        provision_adjustment=float(n.get("provision_adjustment") or 0),
        provision_closing=float(n.get("provision_closing") or 0),
        entries=[DoubtfulDebtEntry(**e) for e in json.loads(n.get("entries_json") or "[]")],
        journal_entry_id=n.get("journal_entry_id"),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


async def add_debtor(
    session: AsyncSession,
    user_id: str,
    *,
    debtor_id: str,
    debtor_name: str,
    invoice_number: str,
    invoice_date: datetime,
    due_date: datetime,
    amount: float,
    risk_assessment: str = "medium",
) -> DoubtfulDebtEntry:
    percent = float(DEFAULT_RISK_POLICIES.get(risk_assessment, 5))
    entry = DoubtfulDebtEntry(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        debtor_id=debtor_id,
        debtor_name=debtor_name,
        invoice_number=invoice_number,
        invoice_date=invoice_date,
        due_date=due_date,
        amount=amount,
        outstanding=amount,
        risk_assessment=risk_assessment,
        provision_percent=percent,
    )
    entry.provision_amount = entry.outstanding * (entry.provision_percent / 100)
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:DoubtfulDebtor {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        debtor_id: $debtor_id,
        debtor_name: $debtor_name,
        invoice_number: $invoice_number,
        invoice_date: datetime($invoice_date),
        due_date: datetime($due_date),
        amount: toFloat($amount),
        outstanding: toFloat($outstanding),
        days_overdue: 0,
        risk_assessment: $risk_assessment,
        provision_percent: toFloat($percent),
        provision_amount: toFloat($provision_amount),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_DOUBTFUL_DEBTOR]->(x)
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
            "invoice_date": _iso(invoice_date),
            "due_date": _iso(due_date),
            "amount": amount,
            "outstanding": entry.outstanding,
            "risk_assessment": risk_assessment,
            "percent": entry.provision_percent,
            "provision_amount": entry.provision_amount,
            "created_at": _now().isoformat(),
        },
    )
    return entry


async def list_debtors(
    session: AsyncSession, user_id: str, risk_assessment: Optional[str] = None
) -> List[DoubtfulDebtEntry]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DOUBTFUL_DEBTOR]->(x:DoubtfulDebtor)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    entries = [_entry_from_node(dict(r["x"]), user_id) async for r in result]
    if risk_assessment:
        entries = [e for e in entries if e.risk_assessment == risk_assessment]
    return entries


async def get_debtor(session: AsyncSession, user_id: str, debtor_id: str) -> DoubtfulDebtEntry:
    """The first record for a debtor id wins, as the list scan did."""
    for entry in await list_debtors(session, user_id):
        if entry.debtor_id == debtor_id:
            return entry
    raise NotFoundError("Debtor not found")


async def assess_debtor(
    session: AsyncSession, user_id: str, debtor_id: str, risk_assessment: str, notes: Optional[str] = None
) -> DoubtfulDebtEntry:
    entry = await get_debtor(session, user_id, debtor_id)
    entry.risk_assessment = risk_assessment
    entry.provision_percent = float(DEFAULT_RISK_POLICIES.get(risk_assessment, 5))
    entry.provision_amount = entry.outstanding * (entry.provision_percent / 100)
    if notes:
        entry.notes = notes

    await _run(
        session,
        """
        MATCH (x:DoubtfulDebtor {id: $id})
        SET x.risk_assessment = $risk,
            x.provision_percent = toFloat($percent),
            x.provision_amount = toFloat($amount),
            x.notes = $notes
        """,
        {
            "id": entry.id,
            "risk": entry.risk_assessment,
            "percent": entry.provision_percent,
            "amount": entry.provision_amount,
            "notes": entry.notes,
        },
    )
    return entry


def _escalate(days_overdue: int, grade: str, percent: float) -> tuple[str, float]:
    """The ageing rules: escalate only, never relax."""
    if days_overdue >= 180:
        return "doubtful", 75.0
    if days_overdue >= 90:
        return "high", 25.0
    if days_overdue >= 60:
        return "medium", 5.0
    return grade, percent


async def calculate_provision(
    session: AsyncSession,
    user_id: str,
    period_end: datetime,
    provision_type: ProvisionType = ProvisionType.SPECIFIC,
) -> Dict[str, Any]:
    entries = await list_debtors(session, user_id)
    total_provision = 0.0

    for entry in entries:
        period = (_as_dt(period_end) or _now()).replace(tzinfo=None)
        due = (_as_dt(entry.due_date) or _now()).replace(tzinfo=None)
        entry.days_overdue = max(0, (period - due).days)
        entry.risk_assessment, entry.provision_percent = _escalate(
            entry.days_overdue, entry.risk_assessment, entry.provision_percent
        )
        entry.provision_amount = entry.outstanding * (entry.provision_percent / 100)
        total_provision += entry.provision_amount

        await _run(
            session,
            """
            MATCH (x:DoubtfulDebtor {id: $id})
            SET x.days_overdue = $days,
                x.risk_assessment = $risk,
                x.provision_percent = toFloat($percent),
                x.provision_amount = toFloat($amount)
            """,
            {
                "id": entry.id,
                "days": entry.days_overdue,
                "risk": entry.risk_assessment,
                "percent": entry.provision_percent,
                "amount": entry.provision_amount,
            },
        )

    return {
        "period_end": _as_dt(period_end) or _now(),
        "provision_type": provision_type,
        "total_debtors": len(entries),
        "total_outstanding": sum(e.outstanding for e in entries),
        "total_provision_required": total_provision,
        "entries": [
            {
                "debtor_id": e.debtor_id,
                "name": e.debtor_name,
                "outstanding": e.outstanding,
                "risk": e.risk_assessment,
                "provision_percent": e.provision_percent,
                "provision_amount": e.provision_amount,
            }
            for e in entries
        ],
    }


async def create_provision(
    session: AsyncSession,
    user_id: str,
    period_end: datetime,
    provision_opening: float = 0,
    provision_type: ProvisionType = ProvisionType.SPECIFIC,
    entries: Optional[List[DoubtfulDebtEntry]] = None,
    journal_entry_id: Optional[str] = None,
) -> DoubtfulDebtProvision:
    entries = entries or []
    required = sum(e.provision_amount for e in entries)
    provision = DoubtfulDebtProvision(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        period_end=period_end,
        provision_type=provision_type,
        total_debtors=len(entries),
        provision_required=required,
        provision_opening=provision_opening,
        provision_adjustment=required - provision_opening,
        provision_closing=required,
        entries=entries,
        journal_entry_id=journal_entry_id,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:DoubtfulDebtProvision {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        period_end: datetime($period_end),
        provision_type: $provision_type,
        total_debtors: toFloat($total_debtors),
        provision_required: toFloat($required),
        provision_opening: toFloat($opening),
        provision_adjustment: toFloat($adjustment),
        provision_closing: toFloat($closing),
        entries_json: $entries_json,
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_DOUBTFUL_PROVISION]->(x)
    RETURN x
    """
    await _run(
        session,
        query,
        {
            "id": provision.id,
            "user_id": user_id,
            "period_end": _iso(period_end),
            "provision_type": provision.provision_type.value,
            "total_debtors": provision.total_debtors,
            "required": provision.provision_required,
            "opening": provision.provision_opening,
            "adjustment": provision.provision_adjustment,
            "closing": provision.provision_closing,
            "entries_json": json.dumps([e.model_dump(mode="json") for e in provision.entries]),
            "journal_entry_id": journal_entry_id,
            "created_at": _now().isoformat(),
        },
    )
    return provision


async def list_provisions(session: AsyncSession, user_id: str, limit: int = 12) -> List[DoubtfulDebtProvision]:
    """The history is the tail of the records, oldest first inside that tail."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DOUBTFUL_PROVISION]->(x:DoubtfulDebtProvision)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    provisions = [_provision_from_node(dict(r["x"]), user_id) async for r in result]
    if limit and limit > 0:
        provisions = provisions[-limit:]
    return provisions
