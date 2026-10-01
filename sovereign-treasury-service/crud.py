"""
Sovereign Treasury Service CRUD Operations

Neo4j-backed persistence for sovereign accounts, debt instruments and
fiscal positions. All records are stamped with book_id; every read
applies the Book filter `WHERE ($book_id IS NULL OR x.book_id = $book_id)`.
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from sovereign_treasury_service.dependencies import book_id_var
from sovereign_treasury_service.exceptions import NotFoundError
from sovereign_treasury_service.models import FiscalPosition, SovereignAccount, SovereignDebt

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
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
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _account_from_node(n: Dict[str, Any], user_id: str) -> SovereignAccount:
    return SovereignAccount(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        country=n["country"],
        account_type=n.get("account_type", ""),
        balance=float(n.get("balance", 0)),
        currency=n.get("currency", "USD"),
        description=n.get("description", ""),
    )


def _debt_from_node(n: Dict[str, Any], user_id: str) -> SovereignDebt:
    return SovereignDebt(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        country=n["country"],
        instrument=n.get("instrument", ""),
        principal=float(n.get("principal", 0)),
        interest_rate=float(n.get("interest_rate", 0)),
        maturity_date=_as_dt(n.get("maturity_date")) or _now(),
        outstanding=float(n.get("outstanding", 0)),
        currency=n.get("currency", "USD"),
    )


def _position_from_node(n: Dict[str, Any], user_id: str) -> FiscalPosition:
    return FiscalPosition(
        user_id=user_id,
        book_id=n.get("book_id"),
        country=n["country"],
        fiscal_year=n.get("fiscal_year", ""),
        total_revenue=float(n.get("total_revenue", 0)),
        total_expenditure=float(n.get("total_expenditure", 0)),
        fiscal_deficit=float(n.get("fiscal_deficit", 0)),
        deficit_to_gdp=float(n.get("deficit_to_gdp", 0)),
        total_debt=float(n.get("total_debt", 0)),
        debt_to_gdp=float(n.get("debt_to_gdp", 0)),
        foreign_reserves=float(n.get("foreign_reserves", 0)),
        months_import_cover=float(n.get("months_import_cover", 0)),
    )


async def create_account(session: AsyncSession, user_id: str, account: SovereignAccount) -> SovereignAccount:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:SovereignAccount {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        country: $country,
        account_type: $account_type,
        balance: toFloat($balance),
        currency: $currency,
        description: $description,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_SOVEREIGN_ACCOUNT]->(x)
    RETURN x
    """
    params = {
        "id": account.id,
        "user_id": user_id,
        "country": account.country,
        "account_type": account.account_type,
        "balance": account.balance,
        "currency": account.currency,
        "description": account.description,
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _account_from_node(dict(records[0]["x"]), user_id)


async def list_accounts(session: AsyncSession, user_id: str, country: str) -> List[SovereignAccount]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SOVEREIGN_ACCOUNT]->(x:SovereignAccount {{country: $country}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, country=country)
    return [_account_from_node(dict(r["x"]), user_id) async for r in result]


async def register_debt(session: AsyncSession, user_id: str, debt: SovereignDebt) -> SovereignDebt:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:SovereignDebt {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        country: $country,
        instrument: $instrument,
        principal: toFloat($principal),
        interest_rate: toFloat($interest_rate),
        maturity_date: datetime($maturity_date),
        outstanding: toFloat($outstanding),
        currency: $currency,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_SOVEREIGN_DEBT]->(x)
    RETURN x
    """
    params = {
        "id": debt.id,
        "user_id": user_id,
        "country": debt.country,
        "instrument": debt.instrument,
        "principal": debt.principal,
        "interest_rate": debt.interest_rate,
        "maturity_date": debt.maturity_date.isoformat(),
        "outstanding": debt.outstanding,
        "currency": debt.currency,
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _debt_from_node(dict(records[0]["x"]), user_id)


async def list_debts(session: AsyncSession, user_id: str, country: str) -> List[SovereignDebt]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SOVEREIGN_DEBT]->(x:SovereignDebt {{country: $country}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, country=country)
    return [_debt_from_node(dict(r["x"]), user_id) async for r in result]


async def _get_stored_position(session: AsyncSession, user_id: str, country: str) -> Optional[FiscalPosition]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_FISCAL_POSITION]->(x:FiscalPosition {{country: $country}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, country=country)
    records = [r async for r in result]
    if not records:
        return None
    return _position_from_node(dict(records[0]["x"]), user_id)


async def set_fiscal_position(session: AsyncSession, user_id: str, pos: FiscalPosition) -> FiscalPosition:
    """Upsert the caller's fiscal position for the country (one node per user+country)."""
    pos.fiscal_deficit = pos.total_expenditure - pos.total_revenue
    stored = await _get_stored_position(session, user_id, pos.country)
    if stored is not None:
        query = f"""
        MATCH (u:User {{id: $user_id}})-[:OWNS_FISCAL_POSITION]->(x:FiscalPosition {{country: $country}})
        {BOOK_FILTER}
        SET x.fiscal_year = $fiscal_year,
            x.total_revenue = toFloat($total_revenue),
            x.total_expenditure = toFloat($total_expenditure),
            x.fiscal_deficit = toFloat($fiscal_deficit),
            x.deficit_to_gdp = toFloat($deficit_to_gdp),
            x.total_debt = toFloat($total_debt),
            x.debt_to_gdp = toFloat($debt_to_gdp),
            x.foreign_reserves = toFloat($foreign_reserves),
            x.months_import_cover = toFloat($months_import_cover)
        RETURN x
        """
    else:
        query = """
        MATCH (u:User {id: $user_id})
        CREATE (x:FiscalPosition {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            country: $country,
            fiscal_year: $fiscal_year,
            total_revenue: toFloat($total_revenue),
            total_expenditure: toFloat($total_expenditure),
            fiscal_deficit: toFloat($fiscal_deficit),
            deficit_to_gdp: toFloat($deficit_to_gdp),
            total_debt: toFloat($total_debt),
            debt_to_gdp: toFloat($debt_to_gdp),
            foreign_reserves: toFloat($foreign_reserves),
            months_import_cover: toFloat($months_import_cover),
            created_at: datetime($created_at)
        })
        CREATE (u)-[:OWNS_FISCAL_POSITION]->(x)
        RETURN x
        """
    params = {
        "id": str(uuid.uuid4()),
        "user_id": user_id,
        "country": pos.country,
        "fiscal_year": pos.fiscal_year,
        "total_revenue": pos.total_revenue,
        "total_expenditure": pos.total_expenditure,
        "fiscal_deficit": pos.fiscal_deficit,
        "deficit_to_gdp": pos.deficit_to_gdp,
        "total_debt": pos.total_debt,
        "debt_to_gdp": pos.debt_to_gdp,
        "foreign_reserves": pos.foreign_reserves,
        "months_import_cover": pos.months_import_cover,
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _position_from_node(dict(records[0]["x"]), user_id)


async def get_fiscal_position(session: AsyncSession, user_id: str, country: str) -> FiscalPosition:
    stored = await _get_stored_position(session, user_id, country)
    if stored is None:
        raise NotFoundError("No fiscal position found")
    return stored
