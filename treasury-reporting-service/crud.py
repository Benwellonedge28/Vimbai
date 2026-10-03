"""
Treasury Reporting Service CRUD Operations

Reports persist in Neo4j, caller-owned (X-User-Id) and Book-gated
(X-Book-ID, verified upstream by the API gateway). Previously the shared
module-level list let any caller read every tenant's treasury reports.
"""

import json
from typing import Dict, List, Optional

from neo4j import AsyncSession
from treasury_reporting_service.dependencies import book_id_var
from treasury_reporting_service.models import TreasuryReport

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session: AsyncSession, query: str, params: Optional[Dict] = None, **kw):
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _as_dt(value) -> Optional[object]:
    if value is None:
        return None
    if hasattr(value, "iso_format"):  # Temporal
        iso = value.iso_format()
        if iso is None or iso == "None":
            return None
        from datetime import datetime, timezone

        dt = datetime.fromisoformat(iso)
        return dt if dt.tzinfo else dt.replace(tzinfo=timezone.utc)
    return value


def _iso(dt) -> Optional[str]:
    if dt is None:
        return None
    if hasattr(dt, "iso_format"):  # Temporal
        return dt.iso_format()
    if getattr(dt, "tzinfo", None) is None:
        from datetime import timezone

        dt = dt.replace(tzinfo=timezone.utc)
    return dt.isoformat()


def _report_from_node(n: Dict) -> TreasuryReport:
    data = n.get("data", {})
    if isinstance(data, str):
        data = json.loads(data) if data else {}
    summary = n.get("summary", {})
    if isinstance(summary, str):
        summary = json.loads(summary) if summary else {}
    return TreasuryReport(
        id=n["id"],
        report_type=n.get("report_type", ""),
        period=n.get("period", ""),
        data=data,
        summary=summary,
        generated_at=_as_dt(n.get("generated_at")),
        generated_by=n.get("generated_by", ""),
    )


async def create_report(session: AsyncSession, user_id: str, r: TreasuryReport) -> TreasuryReport:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:TreasuryReport {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        report_type: $report_type,
        period: $period,
        data: $data,
        summary: $summary,
        generated_at: datetime($generated_at),
        generated_by: $generated_by
    })
    CREATE (u)-[:OWNS_REPORT]->(x)
    RETURN x
    """
    params = {
        "id": r.id,
        "report_type": r.report_type,
        "period": r.period,
        "data": json.dumps(r.data),
        "summary": json.dumps(r.summary),
        "generated_at": _iso(r.generated_at),
        "generated_by": r.generated_by,
    }
    result = await _run(session, query, params, user_id=user_id)
    rec = await result.single()
    return _report_from_node(dict(rec["x"]))


async def get_report(session: AsyncSession, user_id: str, report_id: str) -> Optional[TreasuryReport]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_REPORT]->(x:TreasuryReport)
    WHERE x.id = $report_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, report_id=report_id, user_id=user_id)
    rec = await result.single()
    return _report_from_node(dict(rec["x"])) if rec else None


async def list_reports(session: AsyncSession, user_id: str) -> List[TreasuryReport]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_REPORT]->(x:TreasuryReport)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_report_from_node(dict(rec["x"])) async for rec in result]
