"""
Control Account Reconciliation Service CRUD Operations

Errors and reconciliation reports are nodes, Book-gated, so one caller's control
account breaks and reconciliation history are invisible to another.

QUIRKS preserved:
* the reported variance is always the absolute gap between the amount and the
  expected amount, whatever the caller supplied;
* a reconciliation walks only the pending errors for that control account type, and
  the report carries a snapshot of them;
* a report counts as reconciled when the difference is under one cent;
* the reports listing returns the most recent entries cut from the tail.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from control_account_reconciliation_service.dependencies import book_id_var
from control_account_reconciliation_service.exceptions import NotFoundError
from control_account_reconciliation_service.models import (
    ControlAccountType,
    ErrorType,
    ReconciliationError,
    ReconciliationReport,
)
from neo4j import AsyncSession

ERROR_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


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


def _error_type(raw) -> ErrorType:
    try:
        return ErrorType(raw)
    except ValueError:
        return ErrorType.POSTING_ERROR


def _account_type(raw) -> ControlAccountType:
    try:
        return ControlAccountType(raw)
    except ValueError:
        return ControlAccountType.SALES_LEDGER


def _error_from_node(n: Dict, user_id: str) -> ReconciliationError:
    return ReconciliationError(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        error_type=_error_type(n.get("error_type")),
        control_account_type=_account_type(n.get("control_account_type")),
        transaction_id=n.get("transaction_id"),
        description=n.get("description") or "",
        amount=float(n.get("amount") or 0),
        expected_amount=float(n.get("expected_amount") or 0),
        variance=float(n.get("variance") or 0),
        status=n.get("status") or "pending",
        resolution=n.get("resolution"),
        resolved_by=n.get("resolved_by"),
        resolved_at=_as_dt(n.get("resolved_at")),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _report_from_node(n: Dict, user_id: str) -> ReconciliationReport:
    raw = n.get("errors_json")
    snapshot: List[ReconciliationError] = []
    if raw:
        try:
            snapshot = [ReconciliationError(**item) for item in json.loads(raw)]
        except (TypeError, ValueError):
            snapshot = []
    return ReconciliationReport(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        as_of_date=_as_dt(n.get("as_of_date")) or _now(),
        control_account_type=_account_type(n.get("control_account_type")),
        control_balance=float(n.get("control_balance") or 0),
        ledger_total=float(n.get("ledger_total") or 0),
        difference=float(n.get("difference") or 0),
        error_count=int(n.get("error_count") or 0),
        errors=snapshot,
        status=n.get("status") or "pending",
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


async def create_error(session: AsyncSession, user_id: str, payload: Dict[str, Any]) -> ReconciliationError:
    error = ReconciliationError(**payload)
    error.id = str(uuid.uuid4())
    error.user_id = user_id
    error.book_id = book_id_var.get()
    error.created_at = _now()
    error.variance = abs(error.amount - error.expected_amount)

    await _run(
        session,
        """
        MATCH (u:User {id: $user_id})
        CREATE (x:ReconciliationError {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            error_type: $error_type,
            control_account_type: $control_account_type,
            transaction_id: $transaction_id,
            description: $description,
            amount: toFloat($amount),
            expected_amount: toFloat($expected_amount),
            variance: toFloat($variance),
            status: $status,
            resolution: $resolution,
            resolved_by: $resolved_by,
            created_at: datetime($created_at)
        })
        CREATE (u)-[:OWNS_RECONCILIATION_ERROR]->(x)
        RETURN x
        """,
        {
            "id": error.id,
            "user_id": user_id,
            "error_type": error.error_type.value,
            "control_account_type": error.control_account_type.value,
            "transaction_id": error.transaction_id,
            "description": error.description,
            "amount": error.amount,
            "expected_amount": error.expected_amount,
            "variance": error.variance,
            "status": error.status,
            "resolution": error.resolution,
            "resolved_by": error.resolved_by,
            "created_at": error.created_at.isoformat(),
        },
    )
    return error


async def list_errors(
    session: AsyncSession,
    user_id: str,
    status: Optional[str] = None,
    error_type: Optional[ErrorType] = None,
) -> List[ReconciliationError]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RECONCILIATION_ERROR]->(x:ReconciliationError)
    {ERROR_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    errors = [_error_from_node(dict(r["x"]), user_id) async for r in result]
    if status:
        errors = [e for e in errors if e.status == status]
    if error_type:
        errors = [e for e in errors if e.error_type == error_type]
    return errors


async def resolve_error(
    session: AsyncSession, user_id: str, error_id: str, resolution: str, resolved_by: str
) -> ReconciliationError:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RECONCILIATION_ERROR]->(x:ReconciliationError {{id: $error_id}})
    {ERROR_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, error_id=error_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Error not found")

    resolved_at = _now()
    await _run(
        session,
        """
        MATCH (x:ReconciliationError {id: $error_id})
        SET x.status = $status,
            x.resolution = $resolution,
            x.resolved_by = $resolved_by,
            x.resolved_at = datetime($resolved_at)
        """,
        {
            "error_id": error_id,
            "status": "resolved",
            "resolution": resolution,
            "resolved_by": resolved_by,
            "resolved_at": resolved_at.isoformat(),
        },
    )
    error = _error_from_node(dict(records[0]["x"]), user_id)
    error.status = "resolved"
    error.resolution = resolution
    error.resolved_by = resolved_by
    error.resolved_at = resolved_at
    return error


async def reconcile(
    session: AsyncSession,
    user_id: str,
    account_type: ControlAccountType,
    as_of_date: datetime,
    control_balance: float,
    ledger_total: float,
) -> ReconciliationReport:
    difference = control_balance - ledger_total

    errors = await list_errors(session, user_id)
    errors = [e for e in errors if e.control_account_type == account_type and e.status == "pending"]

    report = ReconciliationReport(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        as_of_date=as_of_date,
        control_account_type=account_type,
        control_balance=control_balance,
        ledger_total=ledger_total,
        difference=difference,
        error_count=len(errors),
        errors=errors,
        status="reconciled" if abs(difference) < 0.01 else "unreconciled",
    )

    await _run(
        session,
        """
        MATCH (u:User {id: $user_id})
        CREATE (x:ReconciliationReport {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            as_of_date: datetime($as_of_date),
            control_account_type: $control_account_type,
            control_balance: toFloat($control_balance),
            ledger_total: toFloat($ledger_total),
            difference: toFloat($difference),
            error_count: $error_count,
            errors_json: $errors_json,
            status: $status,
            created_at: datetime($created_at)
        })
        CREATE (u)-[:OWNS_RECONCILIATION_REPORT]->(x)
        RETURN x
        """,
        {
            "id": report.id,
            "user_id": user_id,
            "as_of_date": as_of_date.isoformat(),
            "control_account_type": account_type.value,
            "control_balance": control_balance,
            "ledger_total": ledger_total,
            "difference": difference,
            "error_count": report.error_count,
            "errors_json": json.dumps([e.model_dump(mode="json") for e in errors]),
            "status": report.status,
            "created_at": report.created_at.isoformat(),
        },
    )
    return report


async def list_reports(session: AsyncSession, user_id: str, limit: int = 10) -> List[ReconciliationReport]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RECONCILIATION_REPORT]->(x:ReconciliationReport)
    {ERROR_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    reports = [_report_from_node(dict(r["x"]), user_id) async for r in result]
    return reports[-limit:]


async def summary(session: AsyncSession, user_id: str) -> Dict[str, Any]:
    errors = await list_errors(session, user_id)
    return {
        "total_errors": len(errors),
        "pending_errors": len([e for e in errors if e.status == "pending"]),
        "resolved_errors": len([e for e in errors if e.status == "resolved"]),
        "total_variance": sum(e.variance for e in errors if e.status == "pending"),
        "by_type": {et.value: len([e for e in errors if e.error_type == et]) for et in ErrorType},
    }
