"""
Financial Integrity Service CRUD Operations

Integrity checks move from the in-memory _checks defaultdict (keyed
by company_id) to Neo4j: :IntegrityCheck nodes via
:OWNS_INTEGRITY_CHECK edges, book_id stamped, Book-gated. The checks
themselves (balance tolerance, SHA-256 hash verification, count
completeness) are pure computations and unchanged; only their
persistence and the report aggregation change. Report semantics are
identical: total/passed/failed counts and pass_rate percentage over
the caller's Book-visible checks.
"""

from datetime import datetime, timezone
from typing import Any, Dict, List

from financial_integrity_service.dependencies import book_id_var
from financial_integrity_service.models import IntegrityCheck
from neo4j import AsyncSession

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


def _check_from_node(n: Dict[str, Any]) -> IntegrityCheck:
    return IntegrityCheck(
        id=n["id"],
        company_id=n["company_id"],
        check_type=n["check_type"],
        entity_type=n.get("entity_type", ""),
        entity_id=n.get("entity_id", ""),
        passed=bool(n.get("passed", False)),
        details=n.get("details", ""),
        hash_before=n.get("hash_before", ""),
        hash_after=n.get("hash_after", ""),
        checked_at=_coerce_dt(n.get("checked_at")),
    )


async def record_check(session: AsyncSession, user_id: str, check: IntegrityCheck) -> IntegrityCheck:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:IntegrityCheck {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        check_type: $check_type,
        entity_type: $entity_type,
        entity_id: $entity_id,
        passed: $passed,
        details: $details,
        hash_before: $hash_before,
        hash_after: $hash_after,
        checked_at: datetime($checked_at)
    }})
    CREATE (u)-[:OWNS_INTEGRITY_CHECK]->(x)
    """
    await _run(
        session,
        query,
        id=check.id,
        user_id=user_id,
        company_id=check.company_id,
        check_type=check.check_type,
        entity_type=check.entity_type,
        entity_id=check.entity_id,
        passed=check.passed,
        details=check.details,
        hash_before=check.hash_before,
        hash_after=check.hash_after,
        checked_at=check.checked_at.isoformat(),
    )
    return check


async def list_checks(session: AsyncSession, user_id: str, company_id: str) -> List[IntegrityCheck]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_INTEGRITY_CHECK]->(x:IntegrityCheck {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    return [_check_from_node(dict(r["x"])) async for r in result]
