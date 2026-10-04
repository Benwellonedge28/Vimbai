"""Alerts Service CRUD operations.

The two durable stores move to Neo4j as caller-owned, Book-stamped nodes:

    alert_rules -> :AlertRule   via :OWNS_RULE
    alerts      -> :AlertRecord via :OWNS_ALERT

The WebSocket ConnectionManager (active connections, per-user
subscriptions) is inherently ephemeral session state and stays in memory
by design.

Rules carry a persisted `last_triggered` node prop (cooldown bookkeeping)
alongside their model fields; trigger_count increments are written back.
Condition / action_config / metadata dicts persist as JSON props.
"""

import json
import uuid
from datetime import date, datetime
from typing import Any, Dict, List, Optional, Type, Union, get_args, get_origin

from alerts_service.dependencies import book_id_var
from alerts_service.models import AlertInDB, AlertRuleInDB
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

RECORDS: Dict[Type, tuple] = {
    AlertRuleInDB: ("AlertRule", "OWNS_RULE"),
    AlertInDB: ("AlertRecord", "OWNS_ALERT"),
}

JSON_FIELDS: Dict[Type, tuple] = {
    AlertRuleInDB: ("condition", "action_config"),
    AlertInDB: ("metadata",),
}


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _new_id() -> str:
    return str(uuid.uuid4())


def _unwrap(model: Type, name: str):
    ann = model.model_fields[name].annotation
    if get_origin(ann) is Union:
        ann = next((a for a in get_args(ann) if a is not type(None)), ann)
    return ann


def _is_datetime(model: Type, name: str) -> bool:
    return _unwrap(model, name) is datetime


def _to_storable(model: Type, obj) -> Dict[str, Any]:
    """Model -> primitive props (JSON for containers)."""
    props: Dict[str, Any] = {}
    json_fields = JSON_FIELDS.get(model, ())
    for name in model.model_fields:
        v = getattr(obj, name)
        if v is None:
            props[name] = None
        elif name in json_fields:
            props[name] = json.dumps(v)
        elif hasattr(v, "value"):  # enum
            props[name] = v.value
        elif isinstance(v, (datetime, date)):
            props[name] = v.isoformat()
        else:
            props[name] = v
    return props


def _coerce_dt(value: Any) -> Optional[datetime]:
    if isinstance(value, datetime):
        return value
    if value is None:
        return None
    if hasattr(value, "iso_format"):
        try:
            return datetime.fromisoformat(value.iso_format())
        except (TypeError, ValueError):
            return None
    if isinstance(value, str):
        try:
            return datetime.fromisoformat(value)
        except ValueError:
            return None
    return None


def _hydrate(model: Type, node: Dict[str, Any]):
    """Node props -> model instance (caller/Book stamps and extras skipped)."""
    kwargs: Dict[str, Any] = {}
    json_fields = JSON_FIELDS.get(model, ())
    for prop, v in node.items():
        if prop not in model.model_fields:
            continue
        if v is None:
            kwargs[prop] = None
        elif prop in json_fields:
            kwargs[prop] = json.loads(v) if isinstance(v, str) else v
        elif _is_datetime(model, prop):
            kwargs[prop] = _coerce_dt(v)
        else:
            kwargs[prop] = v
    return model(**kwargs)


async def create(session: AsyncSession, user_id: str, obj, extra: Dict[str, Any] = None) -> Any:
    """Persist a caller-owned, Book-stamped record and return it."""
    model = type(obj)
    label, edge = RECORDS[model]
    if "id" in model.model_fields and not getattr(obj, "id", None):
        obj.id = _new_id()
    props = _to_storable(model, obj)
    props.update(extra or {})
    parts = [f"{name}: ${name}" for name in props]
    parts.append("user_id: $user_id")
    parts.append("book_id: $book_id")
    literal = ", ".join(parts)
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:{label} {{{literal}}})
    CREATE (u)-[:{edge}]->(x)
    """
    await _run(session, query, user_id=user_id, **props)
    return obj


async def list_all(session: AsyncSession, user_id: str, model: Type) -> List[Any]:
    """All caller-owned, Book-visible records of a type."""
    label, edge = RECORDS[model]
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_hydrate(model, dict(r["x"])) async for r in result]


async def find(session: AsyncSession, user_id: str, model: Type, record_id: str) -> Optional[Any]:
    """Fetch one record plus its extra node props (e.g. last_triggered)."""
    label, edge = RECORDS[model]
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label} {{id: $record_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, record_id=record_id)
    records = [dict(r["x"]) async for r in result]
    if not records:
        return None, {}
    node = records[0]
    extra = {k: v for k, v in node.items() if k not in model.model_fields and k not in ("user_id", "book_id")}
    return _hydrate(model, node), extra


async def update_props(
    session: AsyncSession, user_id: str, model: Type, record_id: str, updates: Dict[str, Any]
) -> bool:
    """Write a set of prop updates back to a caller-owned record."""
    label, edge = RECORDS[model]
    json_fields = JSON_FIELDS.get(model, ())
    params: Dict[str, Any] = {}
    sets = []
    for k, v in updates.items():
        if v is not None and hasattr(v, "value"):  # enum
            v = v.value
        elif isinstance(v, (datetime, date)):
            v = v.isoformat()
        elif k in json_fields:
            v = json.dumps(v)
        params[k] = v
        sets.append(f"x.{k} = ${k}")
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label} {{id: $record_id}})
    {BOOK_FILTER}
    SET {", ".join(sets)}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, record_id=record_id, **params)
    return bool([r async for r in result])


async def delete_where(session: AsyncSession, user_id: str, model: Type, eq: Dict[str, Any]) -> int:
    """DETACH DELETE the caller's records matching eq props; returns whether any were removed."""
    label, edge = RECORDS[model]
    eq_literal = ", ".join(f"{k}: ${k}" for k in eq)
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label} {{{eq_literal}}})
    {BOOK_FILTER}
    DETACH DELETE x
    """
    before = len(await list_all(session, user_id, model))
    await _run(session, query, user_id=user_id, **eq)
    after = len(await list_all(session, user_id, model))
    return before - after
