"""
MultiCloud Service CRUD Operations

Recovery-plan entities move from the in-memory _store defaultdict
(keyed by company_id) to Neo4j: :MultiCloudItem nodes via :OWNS_MULTI_CLOUD_ITEM edges,
book_id stamped, Book-gated. Semantics preserved from the mock:
deleting an item only flips its status to "deleted" (soft delete, the
item stays listed), and updates stamp updated_at. Metrics are
computed over the caller's Book-visible items only.
"""

from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from multi_cloud_service.dependencies import book_id_var
from multi_cloud_service.exceptions import NotFoundError
from multi_cloud_service.models import Entity
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


def _item_from_node(n: Dict[str, Any]) -> Entity:
    config = n.get("config", {})
    if isinstance(config, str):
        try:
            import json

            config = json.loads(config)
        except ValueError:
            config = {}
    return Entity(
        id=n["id"],
        name=n["name"],
        description=n.get("description", ""),
        config=config,
        status=n.get("status", "active"),
        created_at=_coerce_dt(n.get("created_at")),
        updated_at=_coerce_dt(n.get("updated_at")),
    )


async def create_item(session: AsyncSession, user_id: str, company_id: str, item: Entity) -> Entity:
    import json

    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:MultiCloudItem {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        name: $name,
        description: $description,
        config: $config,
        status: $status,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    }})
    CREATE (u)-[:OWNS_MULTI_CLOUD_ITEM]->(x)
    """
    await _run(
        session,
        query,
        id=item.id,
        user_id=user_id,
        company_id=company_id,
        name=item.name,
        description=item.description,
        config=json.dumps(item.config),
        status=item.status,
        created_at=item.created_at.isoformat(),
        updated_at=item.updated_at.isoformat(),
    )
    return item


async def list_items_for_company(session: AsyncSession, user_id: str, company_id: str) -> List[Entity]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_MULTI_CLOUD_ITEM]->(x:MultiCloudItem {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    return [_item_from_node(dict(r["x"])) async for r in result]


async def find_item(session: AsyncSession, user_id: str, item_id: str) -> Optional[Entity]:
    """Return the item if the caller owns it and it is visible in this Book (any company)."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_MULTI_CLOUD_ITEM]->(x:MultiCloudItem {{id: $item_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, item_id=item_id)
    records = [r async for r in result]
    if not records:
        return None
    return _item_from_node(dict(records[0]["x"]))


async def update_item(
    session: AsyncSession,
    user_id: str,
    item_id: str,
    name: Optional[str] = None,
    description: Optional[str] = None,
    status: Optional[str] = None,
) -> None:
    item = await find_item(session, user_id, item_id)
    if item is None:
        raise NotFoundError("Item not found")
    query = "MATCH (x:MultiCloudItem {id: $item_id}) SET x.updated_at = datetime($now)"
    params = {"item_id": item_id, "now": datetime.now(timezone.utc).isoformat()}
    if name is not None:
        query += ", x.name = $name"
        params["name"] = name
    if description is not None:
        query += ", x.description = $description"
        params["description"] = description
    if status is not None:
        query += ", x.status = $status"
        params["status"] = status
    await _run(session, query, params)


async def delete_item(session: AsyncSession, user_id: str, item_id: str) -> None:
    """Soft delete (original semantics): flip status to 'deleted', keep the node."""
    item = await find_item(session, user_id, item_id)
    if item is None:
        raise NotFoundError("Item not found")
    query = "MATCH (x:MultiCloudItem {id: $item_id}) SET x.status = $status"
    await _run(session, query, item_id=item_id, status="deleted")


async def metrics(session: AsyncSession, user_id: str) -> Dict[str, Any]:
    """Totals over the caller's Book-visible items only."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_MULTI_CLOUD_ITEM]->(x:MultiCloudItem)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    records = [dict(r["x"]) async for r in result]
    companies = {r.get("company_id") for r in records}
    return {"total_items": len(records), "companies": len(companies)}
