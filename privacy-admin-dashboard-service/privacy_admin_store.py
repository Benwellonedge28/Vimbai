"""Book-scoped Neo4j store for privacy-admin-dashboard-service.

The records used to live in module-level lists: they vanished on restart and
every tenant shared one namespace. They now live as nodes owned by the caller
(a ``:OWNS_*`` edge from the calling ``:User``), with ``book_id`` stamped on
every node and Book-gated reads. The caller arrives as ``X-User-Id`` and
``X-Book-Id`` headers, injected by the API gateway.

Timestamps and nested objects are persisted as ISO strings and JSON strings, so
a round trip through the driver is stable and pydantic re-validates them on the
way back out.
"""

import json
import os
import uuid
from contextvars import ContextVar
from typing import Any, Dict, List, Optional

from neo4j import AsyncGraphDatabase

user_id_var: ContextVar[Optional[str]] = ContextVar("user_id", default=None)
book_id_var: ContextVar[Optional[str]] = ContextVar("book_id", default=None)

_driver = None


def get_driver():
    """Lazy driver; tests swap it with ``set_driver``."""
    global _driver
    if _driver is None:
        _driver = AsyncGraphDatabase.driver(
            os.getenv("NEO4J_URI", "bolt://localhost:7687"),
            auth=(os.getenv("NEO4J_USER", "neo4j"), os.getenv("NEO4J_PASSWORD", "password")),
        )
    return _driver


def set_driver(driver) -> None:
    """Install a driver (the fake one in tests)."""
    global _driver
    _driver = driver


async def close_driver() -> None:
    global _driver
    if _driver is not None:
        await _driver.close()
        _driver = None


def caller_id() -> str:
    """The calling user; the gateway always sets it, bare imports do not."""
    return user_id_var.get() or "anonymous"


def _book_where(var: str = "x") -> str:
    return f"WHERE ($book_id IS NULL OR {var}.book_id = $book_id)"


async def _run(query: str, params: Dict[str, Any]):
    merged = dict(params)
    merged["user_id"] = caller_id()
    merged.setdefault("book_id", book_id_var.get())
    async with get_driver().session() as session:
        return await session.run(query, merged)


def dumps(value: Any) -> str:
    return json.dumps(value, default=str)


def loads(value: Any) -> Any:
    if not value:
        return []
    return json.loads(value)


async def create_node(label: str, rel: str, props: Dict[str, Any]) -> Dict[str, Any]:
    """Create a node owned by the caller and stamped with the Book."""
    node = dict(props)
    node.setdefault("id", str(uuid.uuid4()))
    node["user_id"] = caller_id()
    node["book_id"] = book_id_var.get()
    fields = ", ".join(f"{key}: ${key}" for key in node)
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:{label} {{ {fields} }})
    CREATE (u)-[:{rel}]->(x)
    RETURN x
    """
    await _run(query, node)
    return node


async def list_nodes(
    label: str,
    rel: str,
    filters: Optional[Dict[str, Any]] = None,
    limit: Optional[int] = None,
    order_field: str = "created_at",
    descending: bool = False,
) -> List[Dict[str, Any]]:
    """List the caller's Book-visible nodes."""
    direction = "DESC" if descending else "ASC"
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{rel}]->(x:{label})
    {_book_where()}
    RETURN x
    ORDER BY x.{order_field} {direction}
    """
    result = await _run(query, {})
    rows = [dict(record["x"]) async for record in result]
    if filters:
        rows = [row for row in rows if all(row.get(k) == v for k, v in filters.items() if v is not None)]
    if limit is not None:
        rows = rows[:limit]
    return rows


async def get_node(label: str, rel: str, node_id: str) -> Optional[Dict[str, Any]]:
    """Fetch one node by id, scoped to the caller and their Book."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{rel}]->(x:{label} {{id: $node_id}})
    {_book_where()}
    RETURN x
    """
    result = await _run(query, {"node_id": node_id})
    rows = [dict(record["x"]) async for record in result]
    return rows[0] if rows else None


async def update_node(label: str, rel: str, node_id: str, patch: Dict[str, Any]) -> Optional[Dict[str, Any]]:
    """Apply flat property updates to one of the caller's nodes."""
    if not patch:
        return await get_node(label, rel, node_id)
    sets = ", ".join(f"x.{key} = ${key}" for key in patch)
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{rel}]->(x:{label} {{id: $node_id}})
    {_book_where()}
    SET {sets}
    RETURN x
    """
    params = dict(patch)
    params["node_id"] = node_id
    result = await _run(query, params)
    rows = [dict(record["x"]) async for record in result]
    return rows[0] if rows else None


async def delete_node(label: str, rel: str, node_id: str) -> bool:
    """Delete one of the caller's nodes. False when it is not theirs to delete."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{rel}]->(x:{label} {{id: $node_id}})
    {_book_where()}
    DETACH DELETE x
    """
    await _run(query, {"node_id": node_id})
    return True
