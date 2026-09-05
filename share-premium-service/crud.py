"""
Share Premium Service CRUD Operations

Neo4j-backed persistence driven generically by the pydantic models:
premium entries, utilizations, and adjustments map to labeled node
types with user ownership (X-User-Id) and book_id stamping. Every read
applies the Book filter `WHERE ($book_id IS NULL OR x.book_id = $book_id)`.

Record-keeping only: this service records share premium movements and
journal-entry references; it never moves money. Corrections use
reversing entries.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional, Type

from neo4j import AsyncSession
from pydantic import BaseModel
from share_premium_service.dependencies import book_id_var
from share_premium_service.exceptions import NotFoundError, ValidationError
from share_premium_service.models import PremiumAdjustment, PremiumUtilization, SharePremiumEntry

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _dt(value) -> Optional[datetime]:
    if value is None:
        return None
    if isinstance(value, datetime):
        return value
    if isinstance(value, str):
        return datetime.fromisoformat(value)
    return datetime.fromisoformat(value.iso_format())


# ============================================================================
# Generic model <-> node mapping (driven by pydantic model_fields)
# ============================================================================

_JSON_FIELDS: Dict[str, set] = {}  # model class name -> fields persisted as JSON strings


def _json_fields(model_cls: Type[BaseModel]) -> set:
    """Fields typed List[...] or Dict[...] are persisted as JSON strings."""
    if model_cls.__name__ not in _JSON_FIELDS:
        fields = set()
        for name, field in model_cls.model_fields.items():
            ann = str(field.annotation)
            if "List" in ann or "Dict" in ann:
                fields.add(name)
        _JSON_FIELDS[model_cls.__name__] = fields
    return _JSON_FIELDS[model_cls.__name__]


def _prop_literal(model_cls: Type[BaseModel]) -> str:
    """Build the Cypher property-map literal for CREATE, with proper wrappers."""
    parts = ["id: $id", "user_id: $user_id", "book_id: $book_id", "created_sync: datetime($created_sync)"]
    for name, field in model_cls.model_fields.items():
        if name in ("id", "user_id", "book_id"):
            continue
        ann = str(field.annotation)
        if "datetime" in ann:
            parts.append(f"{name}: datetime(${name})")
        elif "float" in ann:
            parts.append(f"{name}: toFloat(${name})")
        elif ann.startswith("<class 'int'>"):
            parts.append(f"{name}: toInteger(${name})")
        else:
            parts.append(f"{name}: ${name}")
    return "{\n        " + ",\n        ".join(parts) + "\n    }"


def _params_from_model(model_cls: Type[BaseModel], obj: BaseModel, id_: str, user_id: str) -> Dict[str, Any]:
    """Flatten a model into query params (JSON-encode list/dict fields)."""
    dumped = obj.model_dump(mode="json")
    jsonf = _json_fields(model_cls)
    params = {"id": id_, "user_id": user_id, "created_sync": _now().isoformat()}
    for name, value in dumped.items():
        params[name] = json.dumps(value) if name in jsonf else value
    # the model's own user_id/book_id fields are None at creation; re-stamp
    params["user_id"] = user_id
    params["book_id"] = book_id_var.get()
    return params


def _model_from_node(model_cls: Type[BaseModel], n: Dict[str, Any], user_id: str) -> BaseModel:
    """Rebuild a model instance from a node's properties."""
    field_names = set(model_cls.model_fields)
    kwargs: Dict[str, Any] = {}
    if "id" in field_names:
        kwargs["id"] = n["id"]
    if "report_id" in field_names:
        kwargs["report_id"] = n["id"]
    if "user_id" in field_names:
        kwargs["user_id"] = user_id
    if "book_id" in field_names:
        kwargs["book_id"] = n.get("book_id")
    jsonf = _json_fields(model_cls)
    for name, field in model_cls.model_fields.items():
        if name in ("id", "user_id", "book_id"):
            continue
        value = n.get(name)
        ann = str(field.annotation)
        if "datetime" in ann:
            value = _dt(value)
        elif name in jsonf:
            if isinstance(value, str):
                try:
                    value = json.loads(value) if value else []
                except (TypeError, ValueError):
                    value = []
        kwargs[name] = value
    return model_cls(**kwargs)


# ============================================================================
# Generic store operations
# ============================================================================


async def _create(session, user_id, label: str, edge: str, model_cls, obj) -> BaseModel:
    id_ = getattr(obj, "id", None) or getattr(obj, "report_id", None) or str(uuid.uuid4())
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:{label} {_prop_literal(model_cls)})
    CREATE (u)-[:{edge}]->(x)
    RETURN x
    """
    params = _params_from_model(model_cls, obj, id_, user_id)
    result = await _run(session, query, params)
    record = await result.single()
    return _model_from_node(model_cls, record["x"], user_id)


async def _get(session, user_id, label: str, edge: str, model_cls, id_: str) -> Optional[BaseModel]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label} {{id: $id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, id=id_, user_id=user_id)
    record = await result.single()
    return _model_from_node(model_cls, record["x"], user_id) if record else None


async def _list(session, user_id, label: str, edge: str, model_cls) -> List[BaseModel]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_model_from_node(model_cls, r["x"], user_id) async for r in result]


async def _save(session, user_id, label: str, edge: str, model_cls, obj) -> None:
    """Overwrite all mutable fields of an existing record."""
    set_parts = [f"x.{name} = ${name}" for name in model_cls.model_fields if name not in ("id", "user_id", "book_id")]
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label} {{id: $id}})
    {BOOK_FILTER}
    SET {', '.join(set_parts)}
    RETURN x
    """
    dumped = obj.model_dump(mode="json")
    jsonf = _json_fields(model_cls)
    params = {"id": obj.id, "user_id": user_id}
    for name, value in dumped.items():
        if name in ("id", "user_id", "book_id"):
            continue
        params[name] = json.dumps(value) if name in jsonf else value
    await _run(session, query, params)


# ============================================================================
# Store: Share Premium Entries
# ============================================================================


async def create_entry(session, user_id, entry: SharePremiumEntry) -> SharePremiumEntry:
    return await _create(
        session, user_id, "SharePremiumEntryNode", "OWNS_SHARE_PREMIUM_ENTRY", SharePremiumEntry, entry
    )


async def list_entries(session, user_id) -> List[SharePremiumEntry]:
    return await _list(session, user_id, "SharePremiumEntryNode", "OWNS_SHARE_PREMIUM_ENTRY", SharePremiumEntry)


# ============================================================================
# Store: Premium Utilizations
# ============================================================================


async def create_utilization(session, user_id, utilization: PremiumUtilization) -> PremiumUtilization:
    return await _create(
        session, user_id, "PremiumUtilizationNode", "OWNS_PREMIUM_UTILIZATION", PremiumUtilization, utilization
    )


async def list_utilizations(session, user_id) -> List[PremiumUtilization]:
    return await _list(session, user_id, "PremiumUtilizationNode", "OWNS_PREMIUM_UTILIZATION", PremiumUtilization)


# ============================================================================
# Store: Premium Adjustments
# ============================================================================


async def create_adjustment(session, user_id, adjustment: PremiumAdjustment) -> PremiumAdjustment:
    return await _create(
        session, user_id, "PremiumAdjustmentNode", "OWNS_PREMIUM_ADJUSTMENT", PremiumAdjustment, adjustment
    )


async def list_adjustments(session, user_id) -> List[PremiumAdjustment]:
    return await _list(session, user_id, "PremiumAdjustmentNode", "OWNS_PREMIUM_ADJUSTMENT", PremiumAdjustment)
