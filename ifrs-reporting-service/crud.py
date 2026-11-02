"""
IFRS Reporting Service CRUD Operations

Reports and disclosure notes are nodes, Book-gated, so one caller's IFRS reports,
their figures and their disclosure notes are invisible to another.

QUIRKS preserved:
* only the five known report types are accepted, and the rejection message lists
  them;
* adding a note also appends a short entry to the report's own disclosure index, so
  the report carries the note numbers and titles;
* a report can only be approved from the draft or reviewed state;
* listing the notes for a report id that does not exist answers an empty list
  rather than a not-found.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from ifrs_reporting_service.dependencies import book_id_var
from ifrs_reporting_service.exceptions import ConflictError, NotFoundError
from ifrs_reporting_service.models import DisclosureNote, IFRSReport
from neo4j import AsyncSession

REPORT_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

VALID_REPORT_TYPES = ["balance_sheet", "income_statement", "cash_flow", "equity_changes", "notes"]


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


def _json_value(raw, default):
    if not raw:
        return default
    try:
        return json.loads(raw)
    except (TypeError, ValueError):
        return default


def _report_from_node(n: Dict, user_id: str) -> IFRSReport:
    return IFRSReport(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        report_type=n.get("report_type") or "",
        ifrs_standard=n.get("ifrs_standard") or "",
        period=n.get("period") or "",
        reporting_date=_as_dt(n.get("reporting_date")) or _now(),
        data=_json_value(n.get("data_json"), {}),
        disclosures=_json_value(n.get("disclosures_json"), []),
        status=n.get("status") or "draft",
        created_at=_as_dt(n.get("created_at")) or _now(),
        approved_by=n.get("approved_by") or "",
    )


def _note_from_node(n: Dict, user_id: str) -> DisclosureNote:
    return DisclosureNote(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        report_id=n.get("report_id") or "",
        note_number=int(n.get("note_number") or 0),
        title=n.get("title") or "",
        content=n.get("content") or "",
        ifrs_reference=n.get("ifrs_reference") or "",
    )


async def create_report(
    session: AsyncSession,
    user_id: str,
    *,
    report_type: str,
    ifrs_standard: str,
    period: str,
    reporting_date: datetime,
    data: Optional[Dict[str, Any]] = None,
) -> IFRSReport:
    report = IFRSReport(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        report_type=report_type,
        ifrs_standard=ifrs_standard,
        period=period,
        reporting_date=reporting_date,
        data=dict(data or {}),
    )
    await _run(
        session,
        """
        MATCH (u:User {id: $user_id})
        CREATE (x:IfrsReport {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            report_type: $report_type,
            ifrs_standard: $ifrs_standard,
            period: $period,
            reporting_date: datetime($reporting_date),
            data_json: $data_json,
            disclosures_json: $disclosures_json,
            status: $status,
            approved_by: $approved_by,
            created_at: datetime($created_at)
        })
        CREATE (u)-[:OWNS_IFRS_REPORT]->(x)
        RETURN x
        """,
        {
            "id": report.id,
            "user_id": user_id,
            "report_type": report_type,
            "ifrs_standard": ifrs_standard,
            "period": period,
            "reporting_date": reporting_date.isoformat(),
            "data_json": json.dumps(report.data),
            "disclosures_json": json.dumps([]),
            "status": report.status,
            "approved_by": "",
            "created_at": report.created_at.isoformat(),
        },
    )
    return report


async def list_reports(
    session: AsyncSession,
    user_id: str,
    report_type: Optional[str] = None,
    ifrs_standard: Optional[str] = None,
    status: Optional[str] = None,
) -> List[IFRSReport]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_IFRS_REPORT]->(x:IfrsReport)
    {REPORT_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    reports = [_report_from_node(dict(r["x"]), user_id) async for r in result]
    if report_type:
        reports = [r for r in reports if r.report_type == report_type]
    if ifrs_standard:
        reports = [r for r in reports if r.ifrs_standard == ifrs_standard]
    if status:
        reports = [r for r in reports if r.status == status]
    return reports


async def get_report(session: AsyncSession, user_id: str, report_id: str) -> IFRSReport:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_IFRS_REPORT]->(x:IfrsReport {{id: $report_id}})
    {REPORT_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, report_id=report_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Report not found")
    return _report_from_node(dict(records[0]["x"]), user_id)


async def add_note(
    session: AsyncSession,
    user_id: str,
    report_id: str,
    *,
    note_number: int,
    title: str,
    content: str = "",
    ifrs_reference: str = "",
) -> DisclosureNote:
    report = await get_report(session, user_id, report_id)

    note = DisclosureNote(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        report_id=report_id,
        note_number=note_number,
        title=title,
        content=content,
        ifrs_reference=ifrs_reference,
    )
    await _run(
        session,
        """
        MATCH (u:User {id: $user_id})
        CREATE (x:DisclosureNote {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            report_id: $report_id,
            note_number: $note_number,
            title: $title,
            content: $content,
            ifrs_reference: $ifrs_reference
        })
        CREATE (u)-[:OWNS_DISCLOSURE_NOTE]->(x)
        RETURN x
        """,
        {
            "id": note.id,
            "user_id": user_id,
            "report_id": report_id,
            "note_number": note_number,
            "title": title,
            "content": content,
            "ifrs_reference": ifrs_reference,
        },
    )

    report.disclosures.append({"note_number": note_number, "title": title})
    await _run(
        session,
        """
        MATCH (x:IfrsReport {id: $report_id})
        SET x.disclosures_json = $disclosures_json
        """,
        {"report_id": report_id, "disclosures_json": json.dumps(report.disclosures)},
    )
    return note


async def list_notes(session: AsyncSession, user_id: str, report_id: str) -> List[DisclosureNote]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DISCLOSURE_NOTE]->(x:DisclosureNote {{report_id: $report_id}})
    {REPORT_FILTER}
    RETURN x
    ORDER BY x.note_number ASC
    """
    result = await _run(session, query, user_id=user_id, report_id=report_id)
    return [_note_from_node(dict(r["x"]), user_id) async for r in result]


async def approve_report(session: AsyncSession, user_id: str, report_id: str, approved_by: str) -> IFRSReport:
    report = await get_report(session, user_id, report_id)
    if report.status not in ("draft", "reviewed"):
        raise ConflictError(f"Report cannot be approved from {report.status} state")

    await _run(
        session,
        """
        MATCH (x:IfrsReport {id: $report_id})
        SET x.status = $status, x.approved_by = $approved_by
        """,
        {"report_id": report_id, "status": "approved", "approved_by": approved_by},
    )
    report.status = "approved"
    report.approved_by = approved_by
    return report
