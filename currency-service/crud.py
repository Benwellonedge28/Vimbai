"""
Currency Service CRUD Operations

Neo4j-backed persistence for currencies and exchange rates. Default
currencies and USD-relative rates are lazily seeded per user+Book on first
access (original import-time seeding, now scoped instead of global). The
CurrencyConverter stays pure: it is rebuilt from the caller's visible rates
on every request. Cross-scope access follows the original status codes.
"""

import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

from currency_service.dependencies import book_id_var
from currency_service.exceptions import CurrencyError, NotFoundError
from currency_service.models import (
    DEFAULT_CURRENCIES,
    DEFAULT_RATES,
    Currency,
    CurrencyCreate,
    ExchangeRate,
    ExchangeRateCreate,
)
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
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
        if iso is None or iso == "None":
            return None
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _currency_from_node(n: Dict) -> Currency:
    return Currency(
        code=n["code"],
        name=n["name"],
        symbol=n["symbol"],
        decimal_places=int(n.get("decimal_places", 2)),
        is_active=bool(n.get("is_active", True)),
    )


def _rate_from_node(n: Dict) -> ExchangeRate:
    return ExchangeRate(
        from_currency=n["from_currency"],
        to_currency=n["to_currency"],
        rate=float(n.get("rate", 0.0)),
        effective_date=_as_dt(n.get("effective_date")) or _now(),
        source=n.get("source", "manual"),
        last_updated=_as_dt(n.get("last_updated")) or _now(),
    )


# --- generic reads ---


async def _list_nodes(session: AsyncSession, user_id: str, label: str, edge: str) -> List[Dict]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [dict(r["x"]) async for r in result]


async def _get_node(
    session: AsyncSession, user_id: str, label: str, edge: str, prop: str, value: str
) -> Optional[Dict]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    WHERE x.{prop} = $value AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, value=value)
    record = await result.single()
    return dict(record["x"]) if record else None


async def _set_node_props(
    session: AsyncSession, user_id: str, label: str, edge: str, prop: str, value: str, set_lines: str, params: Dict
) -> Dict:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    WHERE x.{prop} = $value AND ($book_id IS NULL OR x.book_id = $book_id)
    SET {set_lines}
    RETURN x
    """
    merged = dict(params)
    merged.update({"user_id": user_id, "value": value})
    result = await _run(session, query, merged)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Record not found")
    return dict(records[0]["x"])


# --- lazy seeding (original defaults, now per user+Book) ---


async def _seed_defaults(session: AsyncSession, user_id: str) -> None:
    """Seed the original DEFAULT_CURRENCIES / DEFAULT_RATES once per user+Book."""
    if await _list_nodes(session, user_id, "Currency", "OWNS_CURRENCY"):
        return

    now = _now().isoformat()
    for code, data in DEFAULT_CURRENCIES.items():
        query = """
        MATCH (u:User {id: $user_id})
        CREATE (x:Currency {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            code: $code,
            name: $name,
            symbol: $symbol,
            decimal_places: toInteger($decimal_places),
            is_active: $is_active,
            created_at: datetime($created_at)
        })
        CREATE (u)-[:OWNS_CURRENCY]->(x)
        """
        await _run(
            session,
            query,
            {
                "id": str(uuid.uuid4()),
                "code": code,
                "name": data["name"],
                "symbol": data["symbol"],
                "decimal_places": data["decimal_places"],
                "is_active": True,
                "created_at": now,
            },
            user_id=user_id,
        )

    for (from_curr, to_curr), rate in DEFAULT_RATES.items():
        query = """
        MATCH (u:User {id: $user_id})
        CREATE (x:ExchangeRate {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            from_currency: $from_currency,
            to_currency: $to_currency,
            rate: toFloat($rate),
            effective_date: datetime($effective_date),
            source: $source,
            last_updated: datetime($last_updated)
        })
        CREATE (u)-[:OWNS_RATE]->(x)
        """
        await _run(
            session,
            query,
            {
                "id": str(uuid.uuid4()),
                "from_currency": from_curr,
                "to_currency": to_curr,
                "rate": rate,
                "effective_date": now,
                "source": "default",
                "last_updated": now,
            },
            user_id=user_id,
        )


# --- currencies ---


async def list_currencies(session: AsyncSession, user_id: str, active_only: bool = False) -> List[Currency]:
    await _seed_defaults(session, user_id)
    currencies = [_currency_from_node(n) for n in await _list_nodes(session, user_id, "Currency", "OWNS_CURRENCY")]
    if active_only:
        currencies = [c for c in currencies if c.is_active]
    return currencies


async def get_currency(session: AsyncSession, user_id: str, code: str) -> Currency:
    await _seed_defaults(session, user_id)
    node = await _get_node(session, user_id, "Currency", "OWNS_CURRENCY", "code", code)
    if not node:
        raise NotFoundError("Currency not found")
    return _currency_from_node(node)


async def create_currency(session: AsyncSession, user_id: str, payload: CurrencyCreate) -> Currency:
    await _seed_defaults(session, user_id)
    code = payload.code.upper()
    if await _get_node(session, user_id, "Currency", "OWNS_CURRENCY", "code", code):
        raise CurrencyError("Currency already exists", status_code=400)
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:Currency {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        code: $code,
        name: $name,
        symbol: $symbol,
        decimal_places: toInteger($decimal_places),
        is_active: $is_active,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_CURRENCY]->(x)
    RETURN x
    """
    result = await _run(
        session,
        query,
        {
            "id": str(uuid.uuid4()),
            "code": code,
            "name": payload.name,
            "symbol": payload.symbol,
            "decimal_places": payload.decimal_places,
            "is_active": True,
            "created_at": _now().isoformat(),
        },
        user_id=user_id,
    )
    records = [r async for r in result]
    return _currency_from_node(dict(records[0]["x"]))


async def update_currency(session: AsyncSession, user_id: str, code: str, payload: CurrencyCreate) -> Currency:
    node = await _get_node(session, user_id, "Currency", "OWNS_CURRENCY", "code", code)
    if not node:
        raise NotFoundError("Currency not found")
    node = await _set_node_props(
        session,
        user_id,
        "Currency",
        "OWNS_CURRENCY",
        "code",
        code,
        "x.name = $name,\n        x.symbol = $symbol,\n        x.decimal_places = toInteger($decimal_places)",
        {
            "name": payload.name,
            "symbol": payload.symbol,
            "decimal_places": payload.decimal_places,
        },
    )
    return _currency_from_node(node)


async def deactivate_currency(session: AsyncSession, user_id: str, code: str) -> None:
    """Soft delete (original semantics: flips is_active, stays listed)."""
    node = await _get_node(session, user_id, "Currency", "OWNS_CURRENCY", "code", code)
    if not node:
        raise NotFoundError("Currency not found")
    await _set_node_props(
        session,
        user_id,
        "Currency",
        "OWNS_CURRENCY",
        "code",
        code,
        "x.is_active = $is_active",
        {"is_active": False},
    )


# --- exchange rates ---


async def list_rates(
    session: AsyncSession,
    user_id: str,
    from_currency: Optional[str] = None,
    to_currency: Optional[str] = None,
) -> List[ExchangeRate]:
    await _seed_defaults(session, user_id)
    rates = [_rate_from_node(n) for n in await _list_nodes(session, user_id, "ExchangeRate", "OWNS_RATE")]
    if from_currency:
        rates = [r for r in rates if r.from_currency == from_currency.upper()]
    if to_currency:
        rates = [r for r in rates if r.to_currency == to_currency.upper()]
    rates.sort(key=lambda x: x.effective_date, reverse=True)
    return rates


async def latest_rates(session: AsyncSession, user_id: str) -> List[ExchangeRate]:
    rates = await list_rates(session, user_id)
    latest: Dict = {}
    for rate in rates:
        key = (rate.from_currency, rate.to_currency)
        if key not in latest or rate.effective_date > latest[key].effective_date:
            latest[key] = rate
    return list(latest.values())


async def get_rate(session: AsyncSession, user_id: str, from_currency: str, to_currency: str) -> ExchangeRate:
    from_currency, to_currency = from_currency.upper(), to_currency.upper()
    rates = [
        r
        for r in await list_rates(session, user_id)
        if r.from_currency == from_currency and r.to_currency == to_currency
    ]
    if not rates:
        raise NotFoundError(f"No rate found for {from_currency} to {to_currency}")
    return max(rates, key=lambda x: x.effective_date)


async def create_rate(session: AsyncSession, user_id: str, payload: ExchangeRateCreate) -> ExchangeRate:
    await _seed_defaults(session, user_id)  # defaults must predate custom entries
    now = _now()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:ExchangeRate {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        from_currency: $from_currency,
        to_currency: $to_currency,
        rate: toFloat($rate),
        effective_date: datetime($effective_date),
        source: $source,
        last_updated: datetime($last_updated)
    })
    CREATE (u)-[:OWNS_RATE]->(x)
    RETURN x
    """
    result = await _run(
        session,
        query,
        {
            "id": str(uuid.uuid4()),
            "from_currency": payload.from_currency.upper(),
            "to_currency": payload.to_currency.upper(),
            "rate": payload.rate,
            "effective_date": (payload.effective_date or now).isoformat(),
            "source": payload.source,
            "last_updated": now.isoformat(),
        },
        user_id=user_id,
    )
    records = [r async for r in result]
    return _rate_from_node(dict(records[0]["x"]))


async def update_rate(
    session: AsyncSession, user_id: str, from_currency: str, to_currency: str, rate: float, source: Optional[str]
) -> ExchangeRate:
    """Original semantics: creates a NEW rate entry for the pair."""
    now = _now()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:ExchangeRate {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        from_currency: $from_currency,
        to_currency: $to_currency,
        rate: toFloat($rate),
        effective_date: datetime($effective_date),
        source: $source,
        last_updated: datetime($last_updated)
    })
    CREATE (u)-[:OWNS_RATE]->(x)
    RETURN x
    """
    result = await _run(
        session,
        query,
        {
            "id": str(uuid.uuid4()),
            "from_currency": from_currency.upper(),
            "to_currency": to_currency.upper(),
            "rate": rate,
            "effective_date": now.isoformat(),
            "source": source or "manual",
            "last_updated": now.isoformat(),
        },
        user_id=user_id,
    )
    records = [r async for r in result]
    return _rate_from_node(dict(records[0]["x"]))


# --- statistics ---


async def stats(session: AsyncSession, user_id: str) -> Dict:
    currencies = await list_currencies(session, user_id)
    rates = await list_rates(session, user_id)
    return {
        "total_currencies": len(currencies),
        "active_currencies": sum(1 for c in currencies if c.is_active),
        "total_rates": len(rates),
        "latest_rate_date": max(r.effective_date for r in rates).isoformat() if rates else None,
    }
