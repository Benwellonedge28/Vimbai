"""
Family & Community Group Service CRUD Operations

All stores persist in Neo4j, caller-owned (X-User-Id) and Book-gated
(X-Book-ID, verified upstream by the API gateway). Previously the four
shared module-level stores let any caller read/modify any group, join
members to foreign groups, and record contributions against them.
"""

import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

from family_community_group_service.dependencies import book_id_var
from family_community_group_service.models import CommunityGroup, Contribution, GroupMember, PayoutSchedule
from neo4j import AsyncSession

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


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


# --- groups ---


def _group_from_node(n: Dict) -> CommunityGroup:
    return CommunityGroup(
        id=n["id"],
        name=n.get("name", ""),
        description=n.get("description", ""),
        contribution_frequency=n.get("contribution_frequency", "monthly"),
        contribution_amount=float(n.get("contribution_amount", 0.0)),
        member_count=int(n.get("member_count", 0)),
        current_cycle=int(n.get("current_cycle", 1)),
        total_pool=float(n.get("total_pool", 0.0)),
        status=n.get("status", "active"),
        created_at=_as_dt(n.get("created_at")) or _utcnow(),
    )


async def create_group(session: AsyncSession, user_id: str, group: CommunityGroup) -> CommunityGroup:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:CommunityGroup {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        description: $description,
        contribution_frequency: $contribution_frequency,
        contribution_amount: toFloat($contribution_amount),
        member_count: toFloat(0),
        current_cycle: toFloat(1),
        total_pool: toFloat(0),
        status: $status,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_GROUP]->(x)
    RETURN x
    """
    params = {
        "id": group.id,
        "name": group.name,
        "description": group.description,
        "contribution_frequency": group.contribution_frequency,
        "contribution_amount": float(group.contribution_amount),
        "status": group.status,
        "created_at": _iso(group.created_at or _utcnow()),
    }
    result = await _run(session, query, params, user_id=user_id)
    rec = await result.single()
    return _group_from_node(dict(rec["x"]))


async def list_groups(session: AsyncSession, user_id: str) -> List[CommunityGroup]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_GROUP]->(x:CommunityGroup)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_group_from_node(dict(rec["x"])) async for rec in result]


async def get_group(session: AsyncSession, user_id: str, group_id: str) -> Optional[CommunityGroup]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_GROUP]->(x:CommunityGroup)
    WHERE x.id = $group_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, group_id=group_id, user_id=user_id)
    rec = await result.single()
    return _group_from_node(dict(rec["x"])) if rec else None


# --- members ---


def _member_from_node(n: Dict) -> GroupMember:
    return GroupMember(
        id=n["id"],
        name=n.get("name", ""),
        email=n.get("email", ""),
        phone=n.get("phone", ""),
        contribution_amount=float(n.get("contribution_amount", 0.0)),
        joined_at=_as_dt(n.get("joined_at")) or _utcnow(),
        active=bool(n.get("active", True)),
    )


async def add_member(session: AsyncSession, user_id: str, group_id: str, member: GroupMember) -> GroupMember:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:GroupMember {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        group_id: $group_id,
        name: $name,
        email: $email,
        phone: $phone,
        contribution_amount: toFloat($contribution_amount),
        joined_at: datetime($joined_at),
        active: $active
    })
    CREATE (u)-[:OWNS_MEMBER]->(x)
    RETURN x
    """
    params = {
        "id": member.id,
        "group_id": group_id,
        "name": member.name,
        "email": member.email,
        "phone": member.phone,
        "contribution_amount": float(member.contribution_amount),
        "joined_at": _iso(member.joined_at or _utcnow()),
        "active": member.active,
    }
    result = await _run(session, query, params, user_id=user_id)
    rec = await result.single()
    return _member_from_node(dict(rec["x"]))


async def list_members(session: AsyncSession, user_id: str, group_id: str) -> List[GroupMember]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_MEMBER]->(x:GroupMember)
    WHERE x.group_id = $group_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, group_id=group_id, user_id=user_id)
    return [_member_from_node(dict(rec["x"])) async for rec in result]


async def get_member(session: AsyncSession, user_id: str, group_id: str, member_id: str) -> Optional[GroupMember]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_MEMBER]->(x:GroupMember)
    WHERE x.group_id = $group_id AND x.id = $member_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, group_id=group_id, member_id=member_id, user_id=user_id)
    rec = await result.single()
    return _member_from_node(dict(rec["x"])) if rec else None


# --- contributions ---


def _contribution_from_node(n: Dict) -> Contribution:
    return Contribution(
        id=n["id"],
        group_id=n.get("group_id", ""),
        member_id=n.get("member_id", ""),
        amount=float(n.get("amount", 0.0)),
        contribution_date=_as_dt(n.get("contribution_date")) or _utcnow(),
        cycle_number=int(n.get("cycle_number", 1)),
        notes=n.get("notes", ""),
    )


async def create_contribution(
    session: AsyncSession, user_id: str, c: Contribution, new_pool_total: float
) -> Contribution:
    # two statements: the fake harness cannot mix MATCH-then-SET with a CREATE+RETURN
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:GroupContribution {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        group_id: $group_id,
        member_id: $member_id,
        amount: toFloat($amount),
        contribution_date: datetime($contribution_date),
        cycle_number: toFloat($cycle_number),
        notes: $notes
    })
    CREATE (u)-[:OWNS_CONTRIBUTION]->(x)
    RETURN x
    """
    params = {
        "id": c.id,
        "group_id": c.group_id,
        "member_id": c.member_id,
        "amount": float(c.amount),
        "contribution_date": _iso(c.contribution_date or _utcnow()),
        "cycle_number": float(c.cycle_number),
        "notes": c.notes,
        "new_pool_total": float(new_pool_total),
    }
    result = await _run(session, query, params, user_id=user_id)
    rec = await result.single()

    set_query = """
    MATCH (u:User {id: $user_id})-[:OWNS_GROUP]->(g:CommunityGroup)
    WHERE g.id = $group_id AND ($book_id IS NULL OR g.book_id = $book_id)
    SET g.total_pool = toFloat($new_pool_total)
    """
    await _run(session, set_query, group_id=c.group_id, new_pool_total=float(new_pool_total), user_id=user_id)

    return _contribution_from_node(dict(rec["x"]))


async def list_contributions(session: AsyncSession, user_id: str, group_id: str) -> List[Contribution]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CONTRIBUTION]->(x:GroupContribution)
    WHERE x.group_id = $group_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, group_id=group_id, user_id=user_id)
    return [_contribution_from_node(dict(rec["x"])) async for rec in result]


# --- payouts ---


def _payout_from_node(n: Dict) -> PayoutSchedule:
    return PayoutSchedule(
        id=n["id"],
        group_id=n.get("group_id", ""),
        member_id=n.get("member_id", ""),
        cycle_number=int(n.get("cycle_number", 1)),
        payout_amount=float(n.get("payout_amount", 0.0)),
        status=n.get("status", "scheduled"),
        scheduled_date=_as_dt(n.get("scheduled_date")) or _utcnow(),
        paid_at=_as_dt(n.get("paid_at")),
    )


async def list_payouts(session: AsyncSession, user_id: str, group_id: str) -> List[PayoutSchedule]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PAYOUT]->(x:GroupPayout)
    WHERE x.group_id = $group_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, group_id=group_id, user_id=user_id)
    return [_payout_from_node(dict(rec["x"])) async for rec in result]
