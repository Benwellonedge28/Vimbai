"""
Operational Audit Service CRUD Operations

Neo4j-backed persistence for audit engagements and findings. All records
are stamped with book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)`.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from operational_audit_service.dependencies import book_id_var
from operational_audit_service.exceptions import NotFoundError
from operational_audit_service.models import (
    AuditEngagement,
    AuditEngagementCreate,
    AuditFinding,
    AuditFindingCreate,
    AuditStatus,
    FindingSeverity,
)

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
        if iso is None or iso == "None":
            return None
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _engagement_from_node(n: Dict[str, Any], user_id: str) -> AuditEngagement:
    findings = [AuditFinding(**f) for f in json.loads(n.get("findings_json") or "[]")]
    return AuditEngagement(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        audit_type=n.get("audit_type", "operational"),
        title=n["title"],
        scope=n.get("scope", ""),
        objectives=json.loads(n.get("objectives_json") or "[]"),
        start_date=_as_dt(n.get("start_date")) or _now(),
        end_date=_as_dt(n.get("end_date")),
        auditor=n.get("auditor", ""),
        status=n.get("status", "planned"),
        findings=findings,
        summary=n.get("summary", ""),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


async def create_engagement(session: AsyncSession, user_id: str, payload: AuditEngagementCreate) -> AuditEngagement:
    engagement = AuditEngagement(
        id=str(uuid.uuid4()),
        user_id=user_id,
        company_id=payload.company_id,
        audit_type=payload.audit_type,
        title=payload.title,
        scope=payload.scope,
        objectives=list(payload.objectives),
        start_date=payload.start_date or _now(),
        end_date=payload.end_date,
        auditor=payload.auditor,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:OperationalEngagement {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        audit_type: $audit_type,
        title: $title,
        scope: $scope,
        objectives_json: $objectives_json,
        start_date: datetime($start_date),
        end_date: datetime($end_date),
        auditor: $auditor,
        status: $status,
        summary: $summary,
        findings_json: $findings_json,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_OPERATIONAL_ENGAGEMENT]->(x)
    RETURN x
    """
    params = {
        "id": engagement.id,
        "user_id": user_id,
        "company_id": engagement.company_id,
        "audit_type": engagement.audit_type,
        "title": engagement.title,
        "scope": engagement.scope,
        "objectives_json": json.dumps(list(engagement.objectives)),
        "start_date": engagement.start_date.isoformat(),
        "end_date": engagement.end_date.isoformat() if engagement.end_date else None,
        "auditor": engagement.auditor,
        "status": engagement.status.value,
        "summary": engagement.summary,
        "findings_json": json.dumps([]),
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _engagement_from_node(dict(records[0]["x"]), user_id)


async def _get_engagement(session: AsyncSession, user_id: str, engagement_id: str) -> Optional[AuditEngagement]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_OPERATIONAL_ENGAGEMENT]->(x:OperationalEngagement {{id: $engagement_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, engagement_id=engagement_id)
    records = [r async for r in result]
    if not records:
        return None
    return _engagement_from_node(dict(records[0]["x"]), user_id)


async def _write_back(session: AsyncSession, engagement: AuditEngagement):
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_OPERATIONAL_ENGAGEMENT]->(x:OperationalEngagement {{id: $engagement_id}})
    {BOOK_FILTER}
    SET x.status = $status,
        x.summary = $summary,
        x.end_date = datetime($end_date),
        x.findings_json = $findings_json
    RETURN x
    """
    params = {
        "user_id": engagement.user_id,
        "engagement_id": engagement.id,
        "status": engagement.status.value,
        "summary": engagement.summary,
        "end_date": engagement.end_date.isoformat() if engagement.end_date else None,
        "findings_json": json.dumps([f.model_dump(mode="json") for f in engagement.findings]),
    }
    await _run(session, query, params)


async def list_engagements(
    session: AsyncSession, user_id: str, company_id: str, status_filter: str = ""
) -> List[AuditEngagement]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_OPERATIONAL_ENGAGEMENT]->(x:OperationalEngagement {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    engagements = [_engagement_from_node(dict(r["x"]), user_id) async for r in result]
    if status_filter:
        engagements = [e for e in engagements if e.status.value == status_filter]
    return engagements


async def update_status(
    session: AsyncSession, user_id: str, engagement_id: str, status: AuditStatus, summary: str = ""
) -> Dict[str, Any]:
    engagement = await _get_engagement(session, user_id, engagement_id)
    if not engagement:
        raise NotFoundError("Engagement not found")
    engagement.status = status
    if status == AuditStatus.COMPLETED:
        engagement.end_date = _now()
        if summary:
            engagement.summary = summary
    await _write_back(session, engagement)
    return {"id": engagement_id, "status": status.value}


async def add_finding(
    session: AsyncSession, user_id: str, engagement_id: str, payload: AuditFindingCreate
) -> Dict[str, Any]:
    engagement = await _get_engagement(session, user_id, engagement_id)
    if not engagement:
        raise NotFoundError("Engagement not found")
    finding = AuditFinding(
        title=payload.title,
        description=payload.description,
        severity=payload.severity,
        recommendation=payload.recommendation,
        evidence=payload.evidence,
    )
    engagement.findings.append(finding)
    await _write_back(session, engagement)
    return {
        "engagement_id": engagement_id,
        "finding_id": finding.id,
        "severity": finding.severity.value,
    }


async def remediate_finding(
    session: AsyncSession, user_id: str, finding_id: str, remediation_note: str = ""
) -> Dict[str, Any]:
    """Locate the finding among the caller's Book-visible engagements."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_OPERATIONAL_ENGAGEMENT]->(x:OperationalEngagement)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    engagements = [_engagement_from_node(dict(r["x"]), user_id) async for r in result]
    for engagement in engagements:
        for f in engagement.findings:
            if f.id == finding_id:
                f.status = "remediated"
                if remediation_note:
                    f.recommendation = f"{f.recommendation}\n\nRemediation: {remediation_note}"
                await _write_back(session, engagement)
                return {"finding_id": finding_id, "status": "remediated"}
    raise NotFoundError("Finding not found")


async def audit_report(session: AsyncSession, user_id: str, engagement_id: str) -> Dict[str, Any]:
    engagement = await _get_engagement(session, user_id, engagement_id)
    if not engagement:
        raise NotFoundError("Engagement not found")
    critical = sum(1 for f in engagement.findings if f.severity == FindingSeverity.CRITICAL)
    high = sum(1 for f in engagement.findings if f.severity == FindingSeverity.HIGH)
    medium = sum(1 for f in engagement.findings if f.severity == FindingSeverity.MEDIUM)
    low = sum(1 for f in engagement.findings if f.severity == FindingSeverity.LOW)
    return {
        "engagement": engagement,
        "findings_summary": {
            "critical": critical,
            "high": high,
            "medium": medium,
            "low": low,
            "total": len(engagement.findings),
        },
        "open_findings": sum(1 for f in engagement.findings if f.status == "open"),
        "remediated": sum(1 for f in engagement.findings if f.status == "remediated"),
    }
