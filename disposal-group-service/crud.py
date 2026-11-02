"""
Disposal Group Service CRUD Operations

Groups, their assets and their impairment tests are all nodes, Book-gated, so one
caller's held-for-sale groups and their asset detail are invisible to another.

QUIRKS preserved:
* the group totals are added to as assets arrive rather than recounted, and the
  group is saved with the running figures it had in memory;
* impairment is what the group carries less what it would fetch, floored at zero,
  and each test simply replaces the loss recorded on the group;
* listing the assets of a group that does not exist answers an empty list, not a
  404;
* status validation happens before the group is looked up, so an invalid status on
  an unknown group answers 400 rather than 404.
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from disposal_group_service.dependencies import book_id_var
from disposal_group_service.exceptions import NotFoundError
from disposal_group_service.models import DisposalAsset, DisposalGroup, ImpairmentTest
from neo4j import AsyncSession

GROUP_FILTER = "WHERE ($book_id IS NULL OR g.book_id = $book_id)"
ASSET_FILTER = "WHERE ($book_id IS NULL OR a.book_id = $book_id)"
USER_GROUP_MATCH = "MATCH (u:User {id: $user_id})-[:OWNS_DISPOSAL_GROUP]->(g:DisposalGroup)"

VALID_STATUSES = ["held_for_sale", "sold", "withdrawn"]


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


def _iso(value: Optional[datetime]) -> Optional[str]:
    return value.isoformat() if value else None


def _group_from_node(n: Dict, user_id: str) -> DisposalGroup:
    return DisposalGroup(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        name=n.get("name") or "",
        description=n.get("description") or "",
        disposal_method=n.get("disposal_method") or "sale",
        status=n.get("status") or "held_for_sale",
        classification_date=_as_dt(n.get("classification_date")) or _now(),
        expected_disposal_date=_as_dt(n.get("expected_disposal_date")),
        total_carrying_amount=float(n.get("total_carrying_amount") or 0),
        total_fair_value=float(n.get("total_fair_value") or 0),
        impairment_loss=float(n.get("impairment_loss") or 0),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _asset_from_node(n: Dict, user_id: str) -> DisposalAsset:
    return DisposalAsset(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        group_id=n.get("group_id"),
        asset_id=n.get("asset_id") or "",
        asset_name=n.get("asset_name") or "",
        carrying_amount=float(n.get("carrying_amount") or 0),
        fair_value=float(n.get("fair_value") or 0),
        classification=n.get("classification") or "noncurrent_asset",
    )


def _test_from_node(n: Dict, user_id: str) -> ImpairmentTest:
    return ImpairmentTest(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        group_id=n.get("group_id") or "",
        carrying_amount=float(n.get("carrying_amount") or 0),
        fair_value_less_costs=float(n.get("fair_value_less_costs") or 0),
        impairment_amount=float(n.get("impairment_amount") or 0),
        tested_at=_as_dt(n.get("tested_at")) or _now(),
        notes=n.get("notes") or "",
    )


async def get_group(session: AsyncSession, user_id: str, group_id: str) -> DisposalGroup:
    query = f"""
    {USER_GROUP_MATCH} {{id: $group_id}}
    {GROUP_FILTER}
    RETURN g
    """
    result = await _run(session, query, user_id=user_id, group_id=group_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Disposal group not found")
    return _group_from_node(dict(records[0]["g"]), user_id)


async def create_group(
    session: AsyncSession,
    user_id: str,
    *,
    name: str,
    description: str = "",
    disposal_method: str = "sale",
    expected_disposal_date: Optional[datetime] = None,
) -> DisposalGroup:
    group = DisposalGroup(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        name=name,
        description=description,
        disposal_method=disposal_method,
        expected_disposal_date=expected_disposal_date,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (g:DisposalGroup {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        description: $description,
        disposal_method: $disposal_method,
        status: $status,
        classification_date: datetime($classification_date),
        expected_disposal_date: $expected_disposal_date,
        total_carrying_amount: 0.0,
        total_fair_value: 0.0,
        impairment_loss: 0.0,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_DISPOSAL_GROUP]->(g)
    RETURN g
    """
    await _run(
        session,
        query,
        {
            "id": group.id,
            "user_id": user_id,
            "name": name,
            "description": description,
            "disposal_method": disposal_method,
            "status": group.status,
            "classification_date": group.classification_date.isoformat(),
            "expected_disposal_date": _iso(expected_disposal_date),
            "created_at": group.created_at.isoformat(),
        },
    )
    return group


async def list_groups(session: AsyncSession, user_id: str, status: Optional[str] = None) -> List[DisposalGroup]:
    query = f"""
    {USER_GROUP_MATCH}
    {GROUP_FILTER}
    RETURN g
    ORDER BY g.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    groups = [_group_from_node(dict(r["g"]), user_id) async for r in result]
    if status:
        groups = [g for g in groups if g.status == status]
    return groups


async def add_asset(
    session: AsyncSession,
    user_id: str,
    group_id: str,
    *,
    asset_id: str,
    asset_name: str,
    carrying_amount: float,
    fair_value: float = 0.0,
    classification: str = "noncurrent_asset",
) -> DisposalAsset:
    group = await get_group(session, user_id, group_id)

    asset = DisposalAsset(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        group_id=group_id,
        asset_id=asset_id,
        asset_name=asset_name,
        carrying_amount=carrying_amount,
        fair_value=fair_value,
        classification=classification,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (a:DisposalAsset {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        group_id: $group_id,
        asset_id: $asset_id,
        asset_name: $asset_name,
        carrying_amount: toFloat($carrying_amount),
        fair_value: toFloat($fair_value),
        classification: $classification
    })
    CREATE (u)-[:OWNS_DISPOSAL_ASSET]->(a)
    RETURN a
    """
    await _run(
        session,
        query,
        {
            "id": asset.id,
            "user_id": user_id,
            "group_id": group_id,
            "asset_id": asset_id,
            "asset_name": asset_name,
            "carrying_amount": carrying_amount,
            "fair_value": fair_value,
            "classification": classification,
        },
    )
    # the totals are added to as assets arrive, never recounted
    await _run(
        session,
        """
        MATCH (g:DisposalGroup {id: $group_id})
        SET g.total_carrying_amount = $carrying_total,
            g.total_fair_value = $fair_total
        """,
        {
            "group_id": group_id,
            "carrying_total": group.total_carrying_amount + carrying_amount,
            "fair_total": group.total_fair_value + fair_value,
        },
    )
    return asset


async def list_assets(session: AsyncSession, user_id: str, group_id: str) -> List[DisposalAsset]:
    """An unknown or foreign group simply has no assets, as before."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DISPOSAL_ASSET]->(a:DisposalAsset {{group_id: $group_id}})
    {ASSET_FILTER}
    RETURN a
    ORDER BY a.carrying_amount ASC, a.asset_id ASC
    """
    result = await _run(session, query, user_id=user_id, group_id=group_id)
    return [_asset_from_node(dict(r["a"]), user_id) async for r in result]


async def test_impairment(
    session: AsyncSession, user_id: str, group_id: str, fair_value_less_costs: float, notes: str = ""
) -> ImpairmentTest:
    group = await get_group(session, user_id, group_id)
    impairment = max(0, group.total_carrying_amount - fair_value_less_costs)

    test = ImpairmentTest(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        group_id=group_id,
        carrying_amount=group.total_carrying_amount,
        fair_value_less_costs=fair_value_less_costs,
        impairment_amount=impairment,
        notes=notes,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (t:ImpairmentTest {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        group_id: $group_id,
        carrying_amount: toFloat($carrying_amount),
        fair_value_less_costs: toFloat($fv_less_costs),
        impairment_amount: toFloat($impairment),
        tested_at: datetime($tested_at),
        notes: $notes
    })
    CREATE (u)-[:OWNS_IMPAIRMENT_TEST]->(t)
    RETURN t
    """
    await _run(
        session,
        query,
        {
            "id": test.id,
            "user_id": user_id,
            "group_id": group_id,
            "carrying_amount": test.carrying_amount,
            "fv_less_costs": fair_value_less_costs,
            "impairment": impairment,
            "tested_at": test.tested_at.isoformat(),
            "notes": notes,
        },
    )
    await _run(
        session,
        "MATCH (g:DisposalGroup {id: $group_id}) SET g.impairment_loss = $impairment",
        {"group_id": group_id, "impairment": impairment},
    )
    return test


async def update_status(session: AsyncSession, user_id: str, group_id: str, status: str) -> Dict[str, Any]:
    await get_group(session, user_id, group_id)
    await _run(
        session,
        "MATCH (g:DisposalGroup {id: $group_id}) SET g.status = $status",
        {"group_id": group_id, "status": status},
    )
    return {"group_id": group_id, "status": status}
