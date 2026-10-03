"""Audit Compliance Service CRUD operations.

The in-memory stores move to Neo4j as caller-owned, Book-stamped nodes:

    AuditEvent       -> :AuditEvent      via :OWNS_EVENT
    VersionSnapshot  -> :VersionSnapshot via :OWNS_SNAPSHOT

data_lineage and integrity_chain are DERIVED, not stored: lineage is the
caller's events filtered by resource_id, and the integrity chain is the
caller's events ordered by the stamped chain_position prop (append-only,
monotonic per caller).

Checksum stability: event timestamps persist as exact ISO strings (not
datetime() temporals) so a hydrated event produces the identical
canonical dump the checksum was computed from. The audited actor's
user_id is stored as actor_user_id to avoid clashing with the caller
stamp (X-User-Id) the fake harness treats as an ownership prop.
"""

import json
import uuid
from datetime import date, datetime, timezone
from typing import Any, Dict, List, Optional, Type, Union, get_args, get_origin

from audit_compliance_service.dependencies import book_id_var
from audit_compliance_service.models import AuditEvent, VersionSnapshot
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

RECORDS: Dict[Type, tuple] = {
    AuditEvent: ("AuditEvent", "OWNS_EVENT"),
    VersionSnapshot: ("VersionSnapshot", "OWNS_SNAPSHOT"),
}

JSON_FIELDS: Dict[Type, tuple] = {
    AuditEvent: ("action_details", "previous_state", "new_state", "metadata"),
    VersionSnapshot: ("state",),
}

# Datetime fields persisted as exact ISO strings (checksum round-trip stability)
STRING_DT_FIELDS: Dict[Type, tuple] = {
    AuditEvent: ("timestamp",),
    VersionSnapshot: ("changed_at",),
}

# Model field -> node prop renames (avoid the caller-stamp user_id prop)
PROP_ALIASES: Dict[Type, Dict[str, str]] = {
    AuditEvent: {"user_id": "actor_user_id"},
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
    """Model -> primitive props (JSON for containers, ISO strings for datetimes)."""
    props: Dict[str, Any] = {}
    json_fields = JSON_FIELDS.get(model, ())
    string_dt = STRING_DT_FIELDS.get(model, ())
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
        elif name in string_dt:
            props[prop] = v.isoformat()
        elif isinstance(v, (datetime, date)):
            props[prop] = v.isoformat()
        else:
            props[prop] = v
    return props


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


def _hydrate(model: Type, node: Dict[str, Any]):
    """Node props -> model instance (reversing aliases)."""
    kwargs: Dict[str, Any] = {}
    json_fields = JSON_FIELDS.get(model, ())
    string_dt = STRING_DT_FIELDS.get(model, ())
    aliases = PROP_ALIASES.get(model, {})
    reverse = {v: k for k, v in aliases.items()}
    for prop, v in node.items():
        if prop in ("user_id", "book_id"):
            continue  # caller stamp, not model data
        name = reverse.get(prop, prop)
        if name not in model.model_fields:
            continue
        if v is None:
            kwargs[name] = None
        elif name in json_fields:
            kwargs[name] = json.loads(v) if isinstance(v, str) else v
        elif name in string_dt or _is_datetime(model, name):
            kwargs[name] = _coerce_dt(v)
        else:
            kwargs[name] = v
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
    # Caller stamp + Book stamp (the harness treats these as ownership props)
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


async def list_chain(session: AsyncSession, user_id: str) -> List[Any]:
    """The caller's AuditEvents in integrity-chain order (chain_position)."""
    label, edge = RECORDS[AuditEvent]
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    pairs = [(int(dict(r["x"])["chain_position"] or 0), dict(r["x"])) async for r in result]
    pairs.sort(key=lambda p: p[0])
    return [_hydrate(AuditEvent, props) for _pos, props in pairs]


async def find(session: AsyncSession, user_id: str, model: Type, record_id: str) -> Optional[Any]:
    """Fetch one record if the caller owns it and it is Book-visible."""
    label, edge = RECORDS[model]
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label} {{id: $record_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, record_id=record_id)
    records = [r async for r in result]
    if not records:
        return None
    return _hydrate(model, dict(records[0]["x"]))
