"""
Treasury Compliance Service CRUD Operations

Compliance checks move from the in-memory _checks defaultdict (keyed
by company_id) to Neo4j: :ComplianceCheck nodes via :OWNS_CHECK
edges, book_id stamped, Book-gated. Default Basel III / SOX checks
are seeded and persisted per user+company+Book on first access
(preserving the original lazy-seed semantics), so status updates and
remediation notes survive restarts. Summary computation (compliant
count, compliance rate) is unchanged and computed in Python.
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List

from neo4j import AsyncSession
from treasury_compliance_service.dependencies import book_id_var
from treasury_compliance_service.exceptions import NotFoundError
from treasury_compliance_service.models import DEFAULT_CHECKS, ComplianceCheck, ComplianceStatus

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _coerce_dt(value: Any) -> datetime:
    if isinstance(value, datetime):
        return value
    if value is None:
        return datetime.now(timezone.utc)
    if hasattr(value, "iso_format"):
        try:
            return datetime.fromisoformat(value.iso_format())
        except (TypeError, ValueError):
            return datetime.now(timezone.utc)
    if isinstance(value, str):
        try:
            return datetime.fromisoformat(value)
        except ValueError:
            return datetime.now(timezone.utc)
    return datetime.now(timezone.utc)


def _check_from_node(n: Dict[str, Any]) -> ComplianceCheck:
    return ComplianceCheck(
        id=n["id"],
        company_id=n["company_id"],
        check_name=n["check_name"],
        regulation=n["regulation"],
        status=n.get("status", ComplianceStatus.PENDING_REVIEW.value),
        details=n.get("details", ""),
        checked_at=_coerce_dt(n.get("checked_at")),
        remediation=n.get("remediation", ""),
    )


async def _create_check(session: AsyncSession, user_id: str, check: ComplianceCheck) -> None:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:ComplianceCheck {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        check_name: $check_name,
        regulation: $regulation,
        status: $status,
        details: $details,
        checked_at: datetime($checked_at),
        remediation: $remediation
    }})
    CREATE (u)-[:OWNS_CHECK]->(x)
    """
    await _run(
        session,
        query,
        id=check.id,
        user_id=user_id,
        company_id=check.company_id,
        check_name=check.check_name,
        regulation=check.regulation,
        status=check.status.value if isinstance(check.status, ComplianceStatus) else str(check.status),
        details=check.details,
        checked_at=check.checked_at.isoformat(),
        remediation=check.remediation,
    )


async def list_checks(session: AsyncSession, user_id: str, company_id: str, seed: bool = True) -> List[ComplianceCheck]:
    """List the caller's Book-visible checks; lazily seed the defaults on first access.

    seed=False preserves the original report behaviour: a company that
    has never run checks reports "Run compliance checks first" without
    being seeded as a side effect of reading the report.
    """
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CHECK]->(x:ComplianceCheck {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    checks = [_check_from_node(dict(r["x"])) async for r in result]
    if not checks and seed:
        for c in DEFAULT_CHECKS:
            checks.append(
                ComplianceCheck(
                    company_id=company_id,
                    check_name=c["check_name"],
                    regulation=c["regulation"],
                    details=c["description"],
                )
            )
        for check in checks:
            await _create_check(session, user_id, check)
    return checks


async def find_check(session: AsyncSession, user_id: str, check_id: str) -> ComplianceCheck | None:
    """Return the check if the caller owns it and it is visible in this Book."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CHECK]->(x:ComplianceCheck {{id: $check_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, check_id=check_id)
    records = [r async for r in result]
    if not records:
        return None
    return _check_from_node(dict(records[0]["x"]))


async def update_check_status(
    session: AsyncSession, user_id: str, check_id: str, status: ComplianceStatus, remediation: str = ""
) -> ComplianceCheck:
    """Update a check the caller owns and can see in this Book; cross-scope updates 404."""
    check = await find_check(session, user_id, check_id)
    if check is None:
        raise NotFoundError("Check not found")
    query = "MATCH (x:ComplianceCheck {id: $check_id}) SET x.status = $status"
    params = {
        "check_id": check_id,
        "status": status.value if isinstance(status, ComplianceStatus) else str(status),
    }
    if remediation:
        query += ", x.remediation = $remediation"
        params["remediation"] = remediation
    await _run(session, query, params)
    updated = await find_check(session, user_id, check_id)
    return updated if updated is not None else check
