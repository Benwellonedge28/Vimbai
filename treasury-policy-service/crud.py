"""
Treasury Policy Service CRUD Operations

Policies, limits and compliance checks persist in Neo4j, caller-owned
(X-User-Id) and Book-gated (X-Book-ID, verified upstream by the API
gateway). Previously the three shared module-level lists let any caller
create policies against anyone's ids, set limits, and run compliance
checks that mutated foreign limits' utilization.
"""

from typing import Dict, List, Optional

from neo4j import AsyncSession
from treasury_policy_service.dependencies import book_id_var
from treasury_policy_service.models import ComplianceCheck, PolicyLimit, TreasuryPolicy

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


# --- policies ---


def _policy_from_node(n: Dict) -> TreasuryPolicy:
    return TreasuryPolicy(
        id=n["id"],
        name=n.get("name", ""),
        description=n.get("description", ""),
        policy_category=n.get("policy_category", ""),
        version=n.get("version", "1.0"),
        effective_date=_as_dt(n.get("effective_date")),
        review_date=_as_dt(n.get("review_date")),
        approved_by=n.get("approved_by", ""),
        status=n.get("status", "active"),
        created_at=_as_dt(n.get("created_at")),
    )


async def create_policy(session: AsyncSession, user_id: str, p: TreasuryPolicy) -> TreasuryPolicy:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:TreasuryPolicy {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        description: $description,
        policy_category: $policy_category,
        version: $version,
        effective_date: datetime($effective_date),
        review_date: datetime($review_date),
        approved_by: $approved_by,
        status: $status,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_POLICY]->(x)
    RETURN x
    """
    params = {
        "id": p.id,
        "name": p.name,
        "description": p.description,
        "policy_category": p.policy_category,
        "version": p.version,
        "effective_date": _iso(p.effective_date),
        "review_date": _iso(p.review_date),
        "approved_by": p.approved_by,
        "status": p.status,
        "created_at": _iso(p.created_at),
    }
    result = await _run(session, query, params, user_id=user_id)
    rec = await result.single()
    return _policy_from_node(dict(rec["x"]))


async def get_policy(session: AsyncSession, user_id: str, policy_id: str) -> Optional[TreasuryPolicy]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_POLICY]->(x:TreasuryPolicy)
    WHERE x.id = $policy_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, policy_id=policy_id, user_id=user_id)
    rec = await result.single()
    return _policy_from_node(dict(rec["x"])) if rec else None


async def list_policies(session: AsyncSession, user_id: str) -> List[TreasuryPolicy]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_POLICY]->(x:TreasuryPolicy)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_policy_from_node(dict(rec["x"])) async for rec in result]


# --- limits ---


def _limit_from_node(n: Dict) -> PolicyLimit:
    return PolicyLimit(
        id=n["id"],
        policy_id=n.get("policy_id", ""),
        limit_type=n.get("limit_type", ""),
        limit_value=float(n.get("limit_value", 0.0)),
        currency=n.get("currency", "USD"),
        warning_threshold=float(n.get("warning_threshold", 0.8)),
        current_utilization=float(n.get("current_utilization", 0.0)),
    )


async def create_limit(session: AsyncSession, user_id: str, policy_id: str, l: PolicyLimit) -> PolicyLimit:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:PolicyLimit {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        policy_id: $policy_id,
        limit_type: $limit_type,
        limit_value: toFloat($limit_value),
        currency: $currency,
        warning_threshold: toFloat($warning_threshold),
        current_utilization: toFloat(0)
    })
    CREATE (u)-[:OWNS_LIMIT]->(x)
    RETURN x
    """
    params = {
        "id": l.id,
        "policy_id": policy_id,
        "limit_type": l.limit_type,
        "limit_value": float(l.limit_value),
        "currency": l.currency,
        "warning_threshold": float(l.warning_threshold),
    }
    result = await _run(session, query, params, user_id=user_id)
    rec = await result.single()
    return _limit_from_node(dict(rec["x"]))


async def get_limit(session: AsyncSession, user_id: str, limit_id: str) -> Optional[PolicyLimit]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_LIMIT]->(x:PolicyLimit)
    WHERE x.id = $limit_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, limit_id=limit_id, user_id=user_id)
    rec = await result.single()
    return _limit_from_node(dict(rec["x"])) if rec else None


async def list_limits(session: AsyncSession, user_id: str, policy_id: str) -> List[PolicyLimit]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_LIMIT]->(x:PolicyLimit)
    WHERE x.policy_id = $policy_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, policy_id=policy_id, user_id=user_id)
    return [_limit_from_node(dict(rec["x"])) async for rec in result]


async def set_limit_utilization(session: AsyncSession, user_id: str, limit_id: str, utilization: float) -> None:
    """Persist the latest checked value on the limit node (original semantics)."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_LIMIT]->(x:PolicyLimit)
    WHERE x.id = $limit_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.current_utilization = toFloat($utilization)
    """
    await _run(session, query, limit_id=limit_id, utilization=float(utilization), user_id=user_id)


# --- compliance checks ---


def _check_from_node(n: Dict) -> ComplianceCheck:
    return ComplianceCheck(
        id=n["id"],
        policy_id=n.get("policy_id", ""),
        limit_id=n.get("limit_id", ""),
        checked_value=float(n.get("checked_value", 0.0)),
        limit_value=float(n.get("limit_value", 0.0)),
        compliant=bool(n.get("compliant", True)),
        utilization_pct=float(n.get("utilization_pct", 0.0)),
        checked_at=_as_dt(n.get("checked_at")),
        notes=n.get("notes", ""),
    )


async def create_check(session: AsyncSession, user_id: str, c: ComplianceCheck) -> ComplianceCheck:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:ComplianceCheck {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        policy_id: $policy_id,
        limit_id: $limit_id,
        checked_value: toFloat($checked_value),
        limit_value: toFloat($limit_value),
        compliant: $compliant,
        utilization_pct: toFloat($utilization_pct),
        checked_at: datetime($checked_at),
        notes: $notes
    })
    CREATE (u)-[:OWNS_CHECK]->(x)
    RETURN x
    """
    params = {
        "id": c.id,
        "policy_id": c.policy_id,
        "limit_id": c.limit_id,
        "checked_value": float(c.checked_value),
        "limit_value": float(c.limit_value),
        "compliant": bool(c.compliant),
        "utilization_pct": float(c.utilization_pct),
        "checked_at": _iso(c.checked_at),
        "notes": c.notes,
    }
    result = await _run(session, query, params, user_id=user_id)
    rec = await result.single()
    return _check_from_node(dict(rec["x"]))


async def list_checks(session: AsyncSession, user_id: str) -> List[ComplianceCheck]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CHECK]->(x:ComplianceCheck)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_check_from_node(dict(rec["x"])) async for rec in result]
