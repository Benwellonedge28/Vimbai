"""
Org Authorization Engine CRUD Operations

Roles and role assignments persist as Neo4j nodes, caller-owned
(X-User-Id) and Book-gated (X-Book-ID). The dead permissions list
(defined in the original mock but read by no endpoint) was dropped
alongside the conversion.
"""

import json
from datetime import datetime, timezone
from typing import Dict, List, Optional

from neo4j import AsyncSession
from org_authorization_engine.dependencies import book_id_var
from org_authorization_engine.models import Role, UserAssignment

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session: AsyncSession, query: str, params: Optional[Dict] = None, **kw):
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


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


def _iso(dt: Optional[datetime]) -> Optional[str]:
    if dt is None:
        return None
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.isoformat()


# --- roles ---


def _role_from_node(n: Dict) -> Role:
    return Role(
        id=n["id"],
        name=n.get("name", ""),
        description=n.get("description", ""),
        permissions=json.loads(n.get("permissions") or "[]"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


async def create_role(session: AsyncSession, user_id: str, r: Role) -> Role:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:OrgRole {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        description: $description,
        permissions: $permissions,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_ROLE]->(x)
    RETURN x
    """
    params = {
        "id": r.id,
        "name": r.name,
        "description": r.description,
        "permissions": json.dumps(r.permissions),
        "created_at": _iso(r.created_at),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _role_from_node(dict(records[0]["x"]))


async def list_roles(session: AsyncSession, user_id: str) -> List[Role]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ROLE]->(x:OrgRole)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_role_from_node(dict(rec["x"])) async for rec in result]


async def get_role(session: AsyncSession, user_id: str, role_id: str) -> Optional[Role]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ROLE]->(x:OrgRole)
    WHERE x.id = $role_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, role_id=role_id, user_id=user_id)
    record = await result.single()
    return _role_from_node(dict(record["x"])) if record else None


# --- assignments ---


def _assignment_from_node(n: Dict) -> UserAssignment:
    return UserAssignment(
        id=n["id"],
        # subject user is stored under subject_user_id (user_id prop = caller stamp)
        user_id=n.get("subject_user_id", ""),
        org_id=n.get("org_id", ""),
        role_id=n.get("role_id", ""),
        assigned_at=_as_dt(n.get("assigned_at")) or datetime.now(timezone.utc),
        assigned_by=n.get("assigned_by", ""),
    )


async def create_assignment(session: AsyncSession, user_id: str, a: UserAssignment) -> UserAssignment:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:RoleAssignment {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        subject_user_id: $subject_user_id,
        org_id: $org_id,
        role_id: $role_id,
        assigned_at: datetime($assigned_at),
        assigned_by: $assigned_by
    })
    CREATE (u)-[:OWNS_ASSIGNMENT]->(x)
    RETURN x
    """
    params = {
        "id": a.id,
        "subject_user_id": a.user_id,
        "org_id": a.org_id,
        "role_id": a.role_id,
        "assigned_at": _iso(a.assigned_at),
        "assigned_by": a.assigned_by,
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _assignment_from_node(dict(records[0]["x"]))


async def list_assignments(session: AsyncSession, user_id: str) -> List[UserAssignment]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ASSIGNMENT]->(x:RoleAssignment)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_assignment_from_node(dict(rec["x"])) async for rec in result]


async def delete_assignment(session: AsyncSession, user_id: str, assignment_id: str) -> None:
    """Detach and remove the caller's assignment node (existence checked upstream)."""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_ASSIGNMENT]->(x:RoleAssignment)
    WHERE x.id = $assignment_id AND ($book_id IS NULL OR x.book_id = $book_id)
    DETACH DELETE x
    """
    await _run(session, query, assignment_id=assignment_id, user_id=user_id)


async def get_assignment(session: AsyncSession, user_id: str, assignment_id: str) -> Optional[UserAssignment]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ASSIGNMENT]->(x:RoleAssignment)
    WHERE x.id = $assignment_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, assignment_id=assignment_id, user_id=user_id)
    record = await result.single()
    return _assignment_from_node(dict(record["x"])) if record else None
