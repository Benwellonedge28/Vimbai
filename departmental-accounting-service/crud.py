"""Departmental Accounting Service CRUD operations.

Six in-memory stores move to Neo4j as caller-owned, Book-stamped nodes:

    departments           -> :Department          via :OWNS_DEPARTMENT
    allocation_rules      -> :DeptAllocationRule   via :OWNS_ALLOCATION_RULE
    inter_dept_bills      -> :InterDeptBill        via :OWNS_INTER_DEPT_BILL
    cost_pools            -> :CostPool             via :OWNS_COST_POOL
    allocation_results    -> :DeptAllocationResult via :OWNS_ALLOCATION_RESULT
    dept_financials_cache -> :DeptFinancials       via :OWNS_DEPT_FINANCIALS

Decimals persist as exact strings; container fields as JSON props.
allocation_results is stored flat (cost_pool_id prop on each result node)
with latest-wins per pool: rerunning /allocate DETACH DELETEs the caller's
prior results for that pool before writing the new set.
"""

import json
import uuid
from datetime import date, datetime
from decimal import Decimal
from typing import Any, Dict, List, Optional, Type, Union, get_args, get_origin

from departmental_accounting_service.dependencies import book_id_var
from departmental_accounting_service.models import (
    Department,
    DepartmentAllocationResult,
    DepartmentAllocationRule,
    DepartmentCostPool,
    DepartmentFinancials,
    InterDepartmentBilling,
)
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

RECORDS: Dict[Type, tuple] = {
    Department: ("Department", "OWNS_DEPARTMENT"),
    DepartmentAllocationRule: ("DeptAllocationRule", "OWNS_ALLOCATION_RULE"),
    InterDepartmentBilling: ("InterDeptBill", "OWNS_INTER_DEPT_BILL"),
    DepartmentCostPool: ("CostPool", "OWNS_COST_POOL"),
    DepartmentAllocationResult: ("DeptAllocationResult", "OWNS_ALLOCATION_RESULT"),
    DepartmentFinancials: ("DeptFinancials", "OWNS_DEPT_FINANCIALS"),
}

JSON_FIELDS: Dict[Type, tuple] = {
    DepartmentCostPool: ("included_departments", "excluded_departments"),
    DepartmentAllocationResult: ("calculation_details",),
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
    """Model -> primitive props (JSON for containers, strings for Decimals)."""
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
        elif isinstance(v, Decimal):
            props[name] = str(v)
        elif isinstance(v, (datetime, date)):
            props[name] = v.isoformat()
        else:
            props[name] = v
    return props


def _coerce_dt(value: Any) -> datetime:
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
    """Node props -> model instance."""
    kwargs: Dict[str, Any] = {}
    json_fields = JSON_FIELDS.get(model, ())
    for prop, v in node.items():
        if prop not in model.model_fields:
            continue  # caller/Book stamps are not model data
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
    # Caller stamp + Book stamp (ownership props for the data layer)
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


async def update_props(
    session: AsyncSession, user_id: str, model: Type, record_id: str, updates: Dict[str, Any]
) -> Optional[Any]:
    """Write a set of prop updates back to a caller-owned record."""
    label, edge = RECORDS[model]
    json_fields = JSON_FIELDS.get(model, ())
    params: Dict[str, Any] = {}
    sets = []
    for k, v in updates.items():
        if v is not None and hasattr(v, "value"):  # enum
            v = v.value
        elif isinstance(v, Decimal):
            v = str(v)
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
    records = [r async for r in result]
    if not records:
        return None
    return _hydrate(model, dict(records[0]["x"]))


async def delete_where(session: AsyncSession, user_id: str, model: Type, eq: Dict[str, Any]) -> int:
    """DETACH DELETE the caller's records matching eq props; returns count removed."""
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
