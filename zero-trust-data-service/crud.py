"""Zero Trust Data Service CRUD operations.

The two durable stores move to Neo4j as caller-owned, Book-stamped nodes:

    policies -> :AccessPolicy via :OWNS_POLICY
    attempts -> :AccessAttempt via :OWNS_ATTEMPT

AccessAttempt.user_id is the *subject* of the access evaluation, not the
caller, so it is stored under a `subject_user_id` prop to avoid colliding
with the caller-ownership `user_id` stamp the fake harness treats as the
owner. List fields persist as JSON props.
"""

import json
import uuid
from datetime import date, datetime
from typing import Any, Dict, List, Optional, Type, Union, get_args, get_origin

from neo4j import AsyncSession
from zero_trust_data_service.dependencies import book_id_var

from zero_trust_data_service.models import AccessAttempt, AccessPolicy

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

RECORDS: Dict[Type, tuple] = {
    AccessPolicy: ("AccessPolicy", "OWNS_POLICY"),
    AccessAttempt: ("AccessAttempt", "OWNS_ATTEMPT"),
}

JSON_FIELDS: Dict[Type, tuple] = {
    AccessPolicy: ("required_roles", "ip_whitelist"),
    AccessAttempt: ("user_roles",),
}

# model field -> stored prop name (avoids the caller-stamp `user_id` prop)
PROP_ALIASES: Dict[Type, Dict[str, str]] = {
    AccessAttempt: {"user_id": "subject_user_id"},
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
    """Model -> primitive props (JSON for containers, aliased user fields)."""
    props: Dict[str, Any] = {}
    json_fields = JSON_FIELDS.get(model, ())
    aliases = PROP_ALIASES.get(model, {})
    for name in model.model_fields:
        v = getattr(obj, name)
        prop = aliases.get(name, name)
        if v is None:
            props[prop] = None
        elif name in json_fields:
            props[prop] = json.dumps(v)
        elif hasattr(v, "value"):  # enum
            props[prop] = v.value
        elif isinstance(v, (datetime, date)):
            props[prop] = v.isoformat()
        else:
            props[prop] = v
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
    """Node props -> model instance (stamps and extras skipped, aliases reversed)."""
    aliases = PROP_ALIASES.get(model, {})
    reverse = {v: k for k, v in aliases.items()}
    json_fields = JSON_FIELDS.get(model, ())
    kwargs: Dict[str, Any] = {}
    for prop, v in node.items():
        name = reverse.get(prop, prop)
        if name not in model.model_fields:
            continue
        # for aliased models the raw user_id prop is the caller stamp, not data
        if model in PROP_ALIASES and prop in ("user_id", "book_id"):
            continue
        if v is None:
            kwargs[name] = None
        elif name in json_fields:
            kwargs[name] = json.loads(v) if isinstance(v, str) else v
        elif _is_datetime(model, name):
            kwargs[name] = _coerce_dt(v)
        else:
            kwargs[name] = v
    return model(**kwargs)


async def create(session: AsyncSession, user_id: str, obj) -> Any:
    """Persist a caller-owned, Book-stamped record and return it."""
    model = type(obj)
    label, edge = RECORDS[model]
    if "id" in model.model_fields and not getattr(obj, "id", None):
        obj.id = _new_id()
    props = _to_storable(model, obj)
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
    """Fetch one record by id."""
    label, edge = RECORDS[model]
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label} {{id: $record_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, record_id=record_id)
    records = [dict(r["x"]) async for r in result]
    if not records:
        return None
    return _hydrate(model, records[0])


async def delete_where(session: AsyncSession, user_id: str, model: Type, eq: Dict[str, Any]) -> bool:
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
    return before > after
