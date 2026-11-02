"""
Lease Termination Service CRUD Operations

:LeaseTermination via :OWNS_LEASE_TERMINATION and :TerminationSettlement via
:OWNS_TERMINATION_SETTLEMENT, both Book-gated, so one caller's lease exits and
their settlement arithmetic are invisible to another.

QUIRKS preserved:
* the settlement amount is worked out once, at initiation, and stored on the
  termination: remaining payments times the payment amount plus the penalty;
* a settlement may be calculated at any stage of the workflow, and repeated
  attempts are kept as separate records rather than replacing the earlier one;
* approval only from pending and completion only from approved, both answering
  400 rather than 409.
"""

import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

from lease_termination_service.dependencies import book_id_var
from lease_termination_service.exceptions import ConflictError, NotFoundError
from lease_termination_service.models import LeaseTermination, TerminationSettlement
from neo4j import AsyncSession

TERMINATION_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


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


def _termination_from_node(n: Dict, user_id: str) -> LeaseTermination:
    return LeaseTermination(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        lease_id=n.get("lease_id") or "",
        termination_date=_as_dt(n.get("termination_date")) or _now(),
        original_end_date=_as_dt(n.get("original_end_date")) or _now(),
        remaining_payments=int(n.get("remaining_payments") or 0),
        remaining_payment_amount=float(n.get("remaining_payment_amount") or 0),
        early_termination_penalty=float(n.get("early_termination_penalty") or 0),
        settlement_amount=float(n.get("settlement_amount") or 0),
        reason=n.get("reason") or "",
        status=n.get("status") or "pending",
        approved_by=n.get("approved_by") or "",
        created_at=_as_dt(n.get("created_at")) or _now(),
        completed_at=_as_dt(n.get("completed_at")),
    )


def _settlement_from_node(n: Dict, user_id: str) -> TerminationSettlement:
    return TerminationSettlement(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        termination_id=n.get("termination_id") or "",
        remaining_lease_payments=float(n.get("remaining_lease_payments") or 0),
        early_termination_penalty=float(n.get("early_termination_penalty") or 0),
        asset_return_value=float(n.get("asset_return_value") or 0),
        net_settlement=float(n.get("net_settlement") or 0),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


async def create_termination(
    session: AsyncSession,
    user_id: str,
    *,
    lease_id: str,
    termination_date: datetime,
    original_end_date: datetime,
    remaining_payments: int,
    remaining_payment_amount: float,
    early_termination_penalty: float = 0.0,
    reason: str = "",
) -> LeaseTermination:
    settlement = remaining_payments * remaining_payment_amount + early_termination_penalty
    termination = LeaseTermination(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        lease_id=lease_id,
        termination_date=termination_date,
        original_end_date=original_end_date,
        remaining_payments=remaining_payments,
        remaining_payment_amount=remaining_payment_amount,
        early_termination_penalty=early_termination_penalty,
        settlement_amount=settlement,
        reason=reason,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:LeaseTermination {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        lease_id: $lease_id,
        termination_date: datetime($termination_date),
        original_end_date: datetime($original_end_date),
        remaining_payments: $remaining_payments,
        remaining_payment_amount: toFloat($remaining_payment_amount),
        early_termination_penalty: toFloat($early_termination_penalty),
        settlement_amount: toFloat($settlement_amount),
        reason: $reason,
        status: $status,
        approved_by: '',
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_LEASE_TERMINATION]->(x)
    RETURN x
    """
    await _run(
        session,
        query,
        {
            "id": termination.id,
            "user_id": user_id,
            "lease_id": lease_id,
            "termination_date": termination.termination_date.isoformat(),
            "original_end_date": termination.original_end_date.isoformat(),
            "remaining_payments": remaining_payments,
            "remaining_payment_amount": remaining_payment_amount,
            "early_termination_penalty": early_termination_penalty,
            "settlement_amount": settlement,
            "reason": reason,
            "status": termination.status,
            "created_at": termination.created_at.isoformat(),
        },
    )
    return termination


async def list_terminations(
    session: AsyncSession, user_id: str, status: Optional[str] = None
) -> List[LeaseTermination]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_LEASE_TERMINATION]->(x:LeaseTermination)
    {TERMINATION_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    terminations = [_termination_from_node(dict(r["x"]), user_id) async for r in result]
    if status:
        terminations = [t for t in terminations if t.status == status]
    return terminations


async def get_termination(session: AsyncSession, user_id: str, termination_id: str) -> LeaseTermination:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_LEASE_TERMINATION]->(x:LeaseTermination {{id: $termination_id}})
    {TERMINATION_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, termination_id=termination_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Termination not found")
    return _termination_from_node(dict(records[0]["x"]), user_id)


async def calculate_settlement(
    session: AsyncSession, user_id: str, termination_id: str, asset_return_value: float = 0.0
) -> TerminationSettlement:
    term = await get_termination(session, user_id, termination_id)

    remaining_payments_value = term.remaining_payments * term.remaining_payment_amount
    net_settlement = remaining_payments_value + term.early_termination_penalty - asset_return_value

    settlement = TerminationSettlement(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        termination_id=termination_id,
        remaining_lease_payments=remaining_payments_value,
        early_termination_penalty=term.early_termination_penalty,
        asset_return_value=asset_return_value,
        net_settlement=net_settlement,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:TerminationSettlement {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        termination_id: $termination_id,
        remaining_lease_payments: toFloat($remaining_lease_payments),
        early_termination_penalty: toFloat($early_termination_penalty),
        asset_return_value: toFloat($asset_return_value),
        net_settlement: toFloat($net_settlement),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_TERMINATION_SETTLEMENT]->(x)
    RETURN x
    """
    await _run(
        session,
        query,
        {
            "id": settlement.id,
            "user_id": user_id,
            "termination_id": termination_id,
            "remaining_lease_payments": remaining_payments_value,
            "early_termination_penalty": term.early_termination_penalty,
            "asset_return_value": asset_return_value,
            "net_settlement": net_settlement,
            "created_at": settlement.created_at.isoformat(),
        },
    )
    return settlement


async def approve_termination(
    session: AsyncSession, user_id: str, termination_id: str, approved_by: str
) -> Dict[str, str]:
    term = await get_termination(session, user_id, termination_id)
    if term.status != "pending":
        raise ConflictError("Termination is not pending")

    await _run(
        session,
        """
        MATCH (x:LeaseTermination {id: $termination_id})
        SET x.status = $status, x.approved_by = $approved_by
        """,
        {"termination_id": termination_id, "status": "approved", "approved_by": approved_by},
    )
    return {"termination_id": termination_id, "status": "approved"}


async def complete_termination(session: AsyncSession, user_id: str, termination_id: str) -> Dict[str, str]:
    term = await get_termination(session, user_id, termination_id)
    if term.status != "approved":
        raise ConflictError("Termination must be approved first")

    await _run(
        session,
        """
        MATCH (x:LeaseTermination {id: $termination_id})
        SET x.status = $status, x.completed_at = datetime($completed_at)
        """,
        {
            "termination_id": termination_id,
            "status": "completed",
            "completed_at": _now().isoformat(),
        },
    )
    return {"termination_id": termination_id, "status": "completed"}
