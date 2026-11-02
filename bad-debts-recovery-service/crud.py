"""
Bad Debts Recovery Service CRUD Operations

Two module-level lists become nodes: :BadDebtRecord via :OWNS_BAD_DEBT and
:RecoveryJournal via :OWNS_RECOVERY_JOURNAL, both stamped with the caller and the
Book. A debt is looked up by its original id, so that id is the query key, and a
record belonging to another caller reads as missing.
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from bad_debts_recovery_service.dependencies import book_id_var
from bad_debts_recovery_service.exceptions import ConflictError, NotFoundError
from bad_debts_recovery_service.models import RecoveryEntry, RecoveryJournal, RecoveryStatus
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


def _entry_from_node(n: Dict[str, Any], user_id: str) -> RecoveryEntry:
    return RecoveryEntry(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        original_debt_id=n["original_debt_id"],
        debtor_name=n["debtor_name"],
        original_amount=float(n.get("original_amount") or 0),
        written_off_amount=float(n.get("written_off_amount") or 0),
        recovered_amount=float(n.get("recovered_amount") or 0),
        recovery_date=_as_dt(n.get("recovery_date")),
        status=RecoveryStatus(n.get("status") or RecoveryStatus.PENDING.value),
        payment_method=n.get("payment_method"),
        reference=n.get("reference"),
        notes=n.get("notes"),
        journal_entry_id=n.get("journal_entry_id"),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _journal_from_node(n: Dict[str, Any], user_id: str) -> RecoveryJournal:
    return RecoveryJournal(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        recovery_id=n["recovery_id"],
        amount=float(n.get("amount") or 0),
        date=_as_dt(n.get("date")) or _now(),
        description=n.get("description") or "",
        journal_entry_id=n.get("journal_entry_id"),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


async def register_debt(
    session: AsyncSession,
    user_id: str,
    *,
    debt_id: str,
    debtor_name: str,
    original_amount: float,
    written_off_amount: float,
    notes: Optional[str] = None,
) -> RecoveryEntry:
    entry = RecoveryEntry(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        original_debt_id=debt_id,
        debtor_name=debtor_name,
        original_amount=original_amount,
        written_off_amount=written_off_amount,
        notes=notes,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:BadDebtRecord {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        original_debt_id: $original_debt_id,
        debtor_name: $debtor_name,
        original_amount: toFloat($original_amount),
        written_off_amount: toFloat($written_off_amount),
        recovered_amount: toFloat(0),
        status: $status,
        notes: $notes,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_BAD_DEBT]->(x)
    RETURN x
    """
    params = {
        "id": entry.id,
        "user_id": user_id,
        "original_debt_id": entry.original_debt_id,
        "debtor_name": entry.debtor_name,
        "original_amount": entry.original_amount,
        "written_off_amount": entry.written_off_amount,
        "status": entry.status.value,
        "notes": entry.notes,
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)
    return entry


async def get_debt(session: AsyncSession, user_id: str, debt_id: str) -> RecoveryEntry:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_BAD_DEBT]->(x:BadDebtRecord {{original_debt_id: $debt_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, debt_id=debt_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Bad debt record not found")
    return _entry_from_node(dict(records[0]["x"]), user_id)


async def list_debts(
    session: AsyncSession, user_id: str, status: Optional[RecoveryStatus] = None
) -> List[RecoveryEntry]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_BAD_DEBT]->(x:BadDebtRecord)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    entries = [_entry_from_node(dict(r["x"]), user_id) async for r in result]
    if status is not None:
        entries = [e for e in entries if e.status == status]
    return entries


async def record_recovery(
    session: AsyncSession,
    user_id: str,
    *,
    debt_id: str,
    amount: float,
    recovery_date: Optional[datetime] = None,
    payment_method: Optional[str] = None,
    reference: Optional[str] = None,
    journal_entry_id: Optional[str] = None,
) -> tuple[RecoveryEntry, RecoveryJournal]:
    entry = await get_debt(session, user_id, debt_id)
    if entry.status == RecoveryStatus.COMPLETED:
        raise ConflictError("Recovery already completed")

    recovered_on = recovery_date or _now()
    journal = RecoveryJournal(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        recovery_id=debt_id,
        amount=amount,
        date=recovered_on,
        description=f"Recovery from {entry.debtor_name}",
        journal_entry_id=journal_entry_id,
    )

    entry.recovered_amount += amount
    entry.recovery_date = recovered_on
    entry.payment_method = payment_method
    entry.reference = reference
    entry.status = (
        RecoveryStatus.COMPLETED if entry.recovered_amount >= entry.written_off_amount else RecoveryStatus.PARTIAL
    )

    update = """
    MATCH (x:BadDebtRecord {id: $id})
    SET x.recovered_amount = toFloat($recovered_amount),
        x.recovery_date = datetime($recovery_date),
        x.payment_method = $payment_method,
        x.reference = $reference,
        x.status = $status,
        x.journal_entry_id = $journal_entry_id
    """
    await _run(
        session,
        update,
        {
            "id": entry.id,
            "recovered_amount": entry.recovered_amount,
            "recovery_date": _iso(recovered_on),
            "payment_method": payment_method,
            "reference": reference,
            "status": entry.status.value,
            "journal_entry_id": journal_entry_id,
        },
    )

    create_journal = """
    MATCH (u:User {id: $user_id})
    CREATE (j:RecoveryJournal {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        recovery_id: $recovery_id,
        amount: toFloat($amount),
        date: datetime($date),
        description: $description,
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_RECOVERY_JOURNAL]->(j)
    RETURN j
    """
    await _run(
        session,
        create_journal,
        {
            "id": journal.id,
            "user_id": user_id,
            "recovery_id": journal.recovery_id,
            "amount": journal.amount,
            "date": _iso(recovered_on),
            "description": journal.description,
            "journal_entry_id": journal_entry_id,
            "created_at": _now().isoformat(),
        },
    )
    return entry, journal


async def list_journals(session: AsyncSession, user_id: str) -> List[RecoveryJournal]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RECOVERY_JOURNAL]->(x:RecoveryJournal)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    return [_journal_from_node(dict(r["x"]), user_id) async for r in result]


def summarise(entries: List[RecoveryEntry]) -> Dict[str, Any]:
    total_written_off = sum(e.written_off_amount for e in entries)
    total_recovered = sum(e.recovered_amount for e in entries)
    pending = sum(e.written_off_amount - e.recovered_amount for e in entries if e.status != RecoveryStatus.COMPLETED)
    return {
        "total_bad_debts": len(entries),
        "total_written_off": total_written_off,
        "total_recovered": total_recovered,
        "pending_recovery": pending,
        "recovery_rate": (total_recovered / total_written_off * 100) if total_written_off > 0 else 0,
        "status_breakdown": {
            "pending": len([e for e in entries if e.status == RecoveryStatus.PENDING]),
            "partial": len([e for e in entries if e.status == RecoveryStatus.PARTIAL]),
            "completed": len([e for e in entries if e.status == RecoveryStatus.COMPLETED]),
        },
    }
