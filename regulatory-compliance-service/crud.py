"""
Regulatory Compliance Service CRUD Operations

Regulations move from an in-memory _regulations dict to Neo4j:
:Regulation nodes via :OWNS_REGULATION edges, book_id stamped,
Book-gated reads. Dashboard aggregates (status counts, compliance
rate, framework/jurisdiction breakdowns, critical items) are computed
server-side in Python.
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List

from neo4j import AsyncSession
from regulatory_compliance_service.dependencies import book_id_var
from regulatory_compliance_service.exceptions import NotFoundError
from regulatory_compliance_service.models import ComplianceDashboard, RegStatus, Regulation, RegulationCreate

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"
_VALID_STATUS = {s.value for s in RegStatus}


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _reg_from_node(n: Dict[str, Any]) -> Regulation:
    return Regulation(
        id=n["id"],
        company_id=n["company_id"],
        regulation_name=n["regulation_name"],
        jurisdiction=n["jurisdiction"],
        framework=n["framework"],
        requirement=n["requirement"],
        status=RegStatus(n.get("status", "pending_review")),
        last_reviewed=n.get("last_reviewed", ""),
        next_review_due=n.get("next_review_due", ""),
        risk_if_non_compliant=n.get("risk_if_non_compliant", "medium"),
    )


async def create_regulation(session: AsyncSession, user_id: str, payload: RegulationCreate) -> Regulation:
    reg = Regulation(id=str(uuid.uuid4()), **payload.model_dump())
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:Regulation {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        regulation_name: $regulation_name,
        jurisdiction: $jurisdiction,
        framework: $framework,
        requirement: $requirement,
        status: $status,
        last_reviewed: $last_reviewed,
        next_review_due: $next_review_due,
        risk_if_non_compliant: $risk_if_non_compliant,
        created_at: datetime($created_at)
    }})
    CREATE (u)-[:OWNS_REGULATION]->(x)
    RETURN x
    """
    params = {
        "id": reg.id,
        "user_id": user_id,
        "company_id": reg.company_id,
        "regulation_name": reg.regulation_name,
        "jurisdiction": reg.jurisdiction,
        "framework": reg.framework,
        "requirement": reg.requirement,
        "status": reg.status.value,
        "last_reviewed": reg.last_reviewed,
        "next_review_due": reg.next_review_due,
        "risk_if_non_compliant": reg.risk_if_non_compliant,
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)
    return reg


async def list_regulations(
    session: AsyncSession, user_id: str, company_id: str, framework: str = ""
) -> List[Regulation]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_REGULATION]->(x:Regulation {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at DESC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    items = [_reg_from_node(dict(r["x"])) async for r in result]
    if framework:
        items = [r for r in items if r.framework == framework]
    return items


async def update_reg_status(
    session: AsyncSession, user_id: str, company_id: str, reg_id: str, status: str
) -> Dict[str, Any]:
    items = await list_regulations(session, user_id, company_id)
    match = next((r for r in items if r.id == reg_id), None)
    if match is None:
        raise NotFoundError("Regulation not found")
    # invalid statuses keep the current status (original tolerance), stamp review date
    new_status = status if status in _VALID_STATUS else match.status.value
    query = """
    MATCH (x:Regulation {id: $id})
    SET x.status = $status, x.last_reviewed = $last_reviewed
    """
    await _run(
        session,
        query,
        id=reg_id,
        status=new_status,
        last_reviewed=_now().strftime("%Y-%m-%d"),
    )
    return {"updated": True, "regulation_id": reg_id, "status": new_status}


async def get_dashboard(session: AsyncSession, user_id: str, company_id: str) -> ComplianceDashboard:
    items = await list_regulations(session, user_id, company_id)
    compliant = sum(1 for r in items if r.status == RegStatus.COMPLIANT)
    non_compliant = sum(1 for r in items if r.status == RegStatus.NON_COMPLIANT)
    pending = sum(1 for r in items if r.status == RegStatus.PENDING_REVIEW)
    total = len(items)
    rate = (compliant / total * 100) if total else 100

    by_framework = {}
    for r in items:
        fw = r.framework
        if fw not in by_framework:
            by_framework[fw] = {"total": 0, "compliant": 0, "non_compliant": 0}
        by_framework[fw]["total"] += 1
        if r.status == RegStatus.COMPLIANT:
            by_framework[fw]["compliant"] += 1
        elif r.status == RegStatus.NON_COMPLIANT:
            by_framework[fw]["non_compliant"] += 1

    by_jur = {}
    for r in items:
        j = r.jurisdiction
        if j not in by_jur:
            by_jur[j] = {"total": 0, "compliant": 0}
        by_jur[j]["total"] += 1
        if r.status == RegStatus.COMPLIANT:
            by_jur[j]["compliant"] += 1

    critical = [
        {
            "regulation": r.regulation_name,
            "jurisdiction": r.jurisdiction,
            "framework": r.framework,
            "status": r.status.value,
            "risk": r.risk_if_non_compliant,
        }
        for r in items
        if r.risk_if_non_compliant in ("high", "critical") and r.status != RegStatus.COMPLIANT
    ]

    return ComplianceDashboard(
        company_id=company_id,
        total_regulations=total,
        compliant=compliant,
        non_compliant=non_compliant,
        pending=pending,
        compliance_rate=rate,
        by_framework=by_framework,
        by_jurisdiction=by_jur,
        critical_items=critical,
    )
