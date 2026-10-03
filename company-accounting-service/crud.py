"""Company Accounting Service CRUD operations.

The eight in-memory module dicts (companies, shareholders, share_capitals,
capital_transactions, dividends, dividend_payments, retained_earnings,
reserves) move to Neo4j as caller-owned, Book-stamped nodes:

    Company            -> :Company            via :OWNS_COMPANY
    Shareholder        -> :Shareholder        via :OWNS_SHAREHOLDER
    ShareCapital       -> :ShareCapital       via :OWNS_SHARE_CAPITAL
    CapitalTransaction -> :CapitalTransaction via :OWNS_CAPITAL_TX
    Dividend           -> :Dividend           via :OWNS_DIVIDEND
    DividendPayment    -> :DividendPayment    via :OWNS_DIVIDEND_PAYMENT
    RetainedEarnings   -> :RetainedEarnings   via :OWNS_RETAINED_EARNINGS
    Reserve            -> :Reserve            via :OWNS_RESERVE

Every read is Book-gated: a record is visible only to its owner (edge from
the caller) and, when the request carries an X-Book-ID, only if the record
was stamped with that Book. Cross-scope access 404s.

Decimals are persisted as exact strings and hydrated back to Decimal;
datetimes as datetime($param) with ISO strings (fake + real Cypher parity).
"""

import uuid
from datetime import datetime, timezone
from decimal import Decimal, InvalidOperation
from typing import Any, Dict, List, Optional, Type, Union, get_args, get_origin

from company_accounting_service.dependencies import book_id_var
from company_accounting_service.models import (
    CapitalTransaction,
    Company,
    Dividend,
    DividendPayment,
    Reserve,
    RetainedEarnings,
    ShareCapital,
    Shareholder,
)
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

RECORDS: Dict[Type, tuple] = {
    Company: ("Company", "OWNS_COMPANY"),
    Shareholder: ("Shareholder", "OWNS_SHAREHOLDER"),
    ShareCapital: ("ShareCapital", "OWNS_SHARE_CAPITAL"),
    CapitalTransaction: ("CapitalTransaction", "OWNS_CAPITAL_TX"),
    Dividend: ("Dividend", "OWNS_DIVIDEND"),
    DividendPayment: ("DividendPayment", "OWNS_DIVIDEND_PAYMENT"),
    RetainedEarnings: ("RetainedEarnings", "OWNS_RETAINED_EARNINGS"),
    Reserve: ("Reserve", "OWNS_RESERVE"),
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


def _is_datetime(model: Type, name: str) -> bool:
    ann = model.model_fields[name].annotation
    if get_origin(ann) is Union:
        ann = next((a for a in get_args(ann) if a is not type(None)), ann)
    return ann is datetime or ann in (datetime,)


def _is_decimal(model: Type, name: str) -> bool:
    ann = model.model_fields[name].annotation
    if get_origin(ann) is Union:
        ann = next((a for a in get_args(ann) if a is not type(None)), ann)
    return ann is Decimal


def _to_storable(model: Type, obj) -> Dict[str, Any]:
    """Model -> primitive props (Decimals as exact strings, datetimes ISO)."""
    props: Dict[str, Any] = {}
    for name in model.model_fields:
        v = getattr(obj, name)
        if v is None:
            props[name] = None
        elif _is_decimal(model, name):
            props[name] = str(v)
        elif _is_datetime(model, name):
            props[name] = v.isoformat()
        elif hasattr(v, "value"):  # enum
            props[name] = v.value
        else:
            props[name] = v
    return props


def _hydrate(model: Type, node: Dict[str, Any]):
    """Node props -> model instance (Decimal/datetime/enum coercion)."""
    kwargs: Dict[str, Any] = {}
    for name in model.model_fields:
        if name not in node:
            continue
        v = node[name]
        if v is None:
            kwargs[name] = None
        elif _is_decimal(model, name):
            try:
                kwargs[name] = Decimal(str(v))
            except (InvalidOperation, ValueError):
                kwargs[name] = Decimal("0")
        elif _is_datetime(model, name):
            kwargs[name] = _coerce_dt(v)
        else:
            kwargs[name] = v
    return model(**kwargs)


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


async def create(session: AsyncSession, user_id: str, obj) -> Any:
    """Persist a caller-owned, Book-stamped record and return it."""
    model = type(obj)
    label, edge = RECORDS[model]
    if not getattr(obj, "id", None):
        obj.id = _new_id()
    props = _to_storable(model, obj)
    parts = []
    for name in props:
        if _is_datetime(model, name) and props[name] is not None:
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


async def find_or_404(session: AsyncSession, user_id: str, model: Type, record_id: str) -> Any:
    from company_accounting_service.exceptions import NotFoundError

    obj = await find(session, user_id, model, record_id)
    if obj is None:
        label, _edge = RECORDS[model]
        raise NotFoundError(f"{label} not found")
    return obj
