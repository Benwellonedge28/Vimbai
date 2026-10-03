"""Accounting Standards Service CRUD operations.

The four in-memory module dicts move to Neo4j as caller-owned,
Book-stamped nodes:

    StandardConfiguration -> :StandardsConfig   via :OWNS_STANDARDS_CONFIG
    AccountMapping         -> :AccountMapping    via :OWNS_ACCOUNT_MAPPING
    AccountingPolicy       -> :AccountingPolicy  via :OWNS_POLICY
    ComplianceCheck        -> :ComplianceCheck   via :OWNS_COMPLIANCE_CHECK

Upsert semantics from the original contract are kept via
delete-then-create (config per organization, policy per
org+standard+area, compliance history per organization). List-valued
fields (selected_standards, alternative_methods, allowed_balances) ride
as JSON props. Dates persist as ISO strings; datetimes as datetime($p).
"""

import json
import uuid
from datetime import date, datetime, timezone
from typing import Any, Dict, List, Optional, Type, Union, get_args, get_origin

from accounting_standards_service.dependencies import book_id_var
from accounting_standards_service.models import (
    AccountingPolicy,
    AccountMapping,
    ComplianceCheck,
    StandardConfiguration,
)
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

RECORDS: Dict[Type, tuple] = {
    StandardConfiguration: ("StandardsConfig", "OWNS_STANDARDS_CONFIG"),
    AccountMapping: ("AccountMapping", "OWNS_ACCOUNT_MAPPING"),
    AccountingPolicy: ("AccountingPolicy", "OWNS_POLICY"),
    ComplianceCheck: ("ComplianceCheck", "OWNS_COMPLIANCE_CHECK"),
}

JSON_FIELDS: Dict[Type, tuple] = {
    StandardConfiguration: ("selected_standards",),
    AccountMapping: ("allowed_balances",),
    AccountingPolicy: ("alternative_methods",),
}


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _new_id() -> str:
    return str(uuid.uuid4())


def _unwrap(model: Type, name: str):
    ann = model.model_fields[name].annotation
    if get_origin(ann) is Union:
        ann = next((a for a in get_args(ann) if a is not type(None)), ann)
    return ann


def _is_datetime(model: Type, name: str) -> bool:
    return _unwrap(model, name) is datetime


def _is_date(model: Type, name: str) -> bool:
    return _unwrap(model, name) is date


def _to_storable(model: Type, obj) -> Dict[str, Any]:
    """Model -> primitive props (JSON for lists, ISO for dates)."""
    props: Dict[str, Any] = {}
    json_fields = JSON_FIELDS.get(model, ())
    for name in model.model_fields:
        v = getattr(obj, name)
        if v is None:
            props[name] = None
        elif name in json_fields:
            props[name] = json.dumps([x.value if hasattr(x, "value") else x for x in v])
        elif hasattr(v, "value"):  # enum
            props[name] = v.value
        elif isinstance(v, date):
            props[name] = v.isoformat()
        elif isinstance(v, datetime):
            props[name] = v.isoformat()
        else:
            props[name] = v
    return props


def _coerce_dt(value: Any) -> datetime:
    if isinstance(value, datetime):
        return value
    if value is None:
        return _now()
    if hasattr(value, "iso_format"):
        try:
            return datetime.fromisoformat(value.iso_format())
        except (TypeError, ValueError):
            return _now()
    if isinstance(value, str):
        try:
            return datetime.fromisoformat(value)
        except ValueError:
            return _now()
    return _now()


def _coerce_date(value: Any) -> date:
    if isinstance(value, date) and not isinstance(value, datetime):
        return value
    if value is None:
        return date(2024, 1, 1)
    if hasattr(value, "iso_format"):
        try:
            return date.fromisoformat(value.iso_format()[:10])
        except (TypeError, ValueError):
            return date(2024, 1, 1)
    if isinstance(value, str):
        try:
            return date.fromisoformat(value[:10])
        except ValueError:
            return date(2024, 1, 1)
    return date(2024, 1, 1)


def _hydrate(model: Type, node: Dict[str, Any]):
    """Node props -> model instance."""
    kwargs: Dict[str, Any] = {}
    json_fields = JSON_FIELDS.get(model, ())
    for name in model.model_fields:
        if name not in node:
            continue
        v = node[name]
        if v is None:
            kwargs[name] = None
        elif name in json_fields:
            kwargs[name] = json.loads(v) if isinstance(v, str) else v
        elif _is_datetime(model, name):
            kwargs[name] = _coerce_dt(v)
        elif _is_date(model, name):
            kwargs[name] = _coerce_date(v)
        else:
            kwargs[name] = v
    return model(**kwargs)


async def create(session: AsyncSession, user_id: str, obj, extra: Dict[str, Any] = None) -> Any:
    """Persist a caller-owned, Book-stamped record and return it.

    `extra` props (e.g. organization_id on models whose original contract
    kept the association only in the dict key) are stamped onto the node.
    """
    model = type(obj)
    label, edge = RECORDS[model]
    if "id" in model.model_fields and not getattr(obj, "id", None):
        obj.id = _new_id()
    props = _to_storable(model, obj)
    props.update(extra or {})
    parts = []
    for name in props:
        if name in (extra or {}):
            parts.append(f"{name}: ${name}")
        elif _is_datetime(model, name) and props[name] is not None:
            parts.append(f"{name}: datetime(${name})")
        else:
            parts.append(f"{name}: ${name}")
    literal = ", ".join(parts)
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:{label} {{{literal}, user_id: $user_id, book_id: $book_id}})
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


async def list_where(session: AsyncSession, user_id: str, model: Type, **eq) -> List[Any]:
    """Caller-owned, Book-visible records whose nodes match the given props.

    Use for associations that live as node props outside the model's
    fields (e.g. organization_id on ComplianceCheck).
    """
    label, edge = RECORDS[model]
    match_props = ", ".join(f"{k}: ${k}" for k in eq)
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label} {{{match_props}}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, **eq)
    return [_hydrate(model, dict(r["x"])) async for r in result]


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


async def update(session: AsyncSession, user_id: str, obj) -> None:
    """Write back all fields of an existing caller-owned record."""
    model = type(obj)
    label, _edge = RECORDS[model]
    props = _to_storable(model, obj)
    sets = []
    for name in props:
        if _is_datetime(model, name) and props[name] is not None:
            sets.append(f"x.{name} = datetime(${name})")
        else:
            sets.append(f"x.{name} = ${name}")
    set_clause = ",\n        ".join(sets)
    query = f"""
    MATCH (x:{label} {{id: $record_id, user_id: $user_id}})
    {BOOK_FILTER}
    SET {set_clause}
    """
    await _run(session, query, record_id=obj.id, user_id=user_id, **props)


async def delete_where(session: AsyncSession, user_id: str, model: Type, **eq) -> None:
    """Delete all caller-owned, Book-visible records matching the given props."""
    label, edge = RECORDS[model]
    match_props = ", ".join(f"{k}: ${k}" for k in eq)
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label} {{{match_props}}})
    {BOOK_FILTER}
    DETACH DELETE x
    """
    await _run(session, query, user_id=user_id, **eq)


async def replace_for(session: AsyncSession, user_id: str, obj, **eq) -> Any:
    """Upsert: delete the caller's matching records, then create the new one."""
    model = type(obj)
    await delete_where(session, user_id, model, **eq)
    return await create(session, user_id, obj)
