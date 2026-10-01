"""
Treasury Management Service CRUD Operations

Neo4j-backed persistence for cash flows and cash positions. All records
are stamped with book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)`.
"""

import uuid
from datetime import datetime, timedelta, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from treasury_management_service.dependencies import book_id_var
from treasury_management_service.models import (
    CashFlowEntry,
    CashFlowEntryCreate,
    CashFlowForecast,
    CashFlowType,
    CashPosition,
    CashPositionUpdate,
    LiquidityLevel,
)

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
    """Coerce a stored temporal/string value into an aware datetime."""
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


def assess_liquidity(cash: float, monthly_burn: float) -> LiquidityLevel:
    if monthly_burn <= 0:
        return LiquidityLevel.EXCESS
    months_runway = cash / monthly_burn
    if months_runway > 6:
        return LiquidityLevel.EXCESS
    if months_runway > 3:
        return LiquidityLevel.ADEQUATE
    if months_runway > 1:
        return LiquidityLevel.TIGHT
    return LiquidityLevel.CRITICAL


def _entry_from_node(n: Dict[str, Any], user_id: str) -> CashFlowEntry:
    return CashFlowEntry(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        account_id=n.get("account_id", ""),
        flow_type=n["flow_type"],
        amount=float(n.get("amount", 0)),
        currency=n.get("currency", "USD"),
        date=_as_dt(n.get("date")) or _now(),
        description=n.get("description", ""),
        category=n.get("category", ""),
    )


def _position_from_node(n: Dict[str, Any], user_id: str) -> CashPosition:
    return CashPosition(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        total_cash=float(n.get("total_cash", 0)),
        currency=n.get("currency", "USD"),
        available_cash=float(n.get("available_cash", 0)),
        restricted_cash=float(n.get("restricted_cash", 0)),
        short_term_investments=float(n.get("short_term_investments", 0)),
        liquidity_level=n.get("liquidity_level", "adequate"),
        as_of=_as_dt(n.get("as_of")) or _now(),
    )


async def record_cashflow(session: AsyncSession, user_id: str, payload: CashFlowEntryCreate) -> CashFlowEntry:
    entry_id = str(uuid.uuid4())
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:CashFlowEntry {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        account_id: $account_id,
        flow_type: $flow_type,
        amount: toFloat($amount),
        currency: $currency,
        date: datetime($date),
        description: $description,
        category: $category,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_CASHFLOW]->(x)
    RETURN x
    """
    params = {
        "id": entry_id,
        "user_id": user_id,
        "company_id": payload.company_id,
        "account_id": payload.account_id,
        "flow_type": payload.flow_type.value,
        "amount": payload.amount,
        "currency": payload.currency,
        "date": (payload.date or _now()).isoformat(),
        "description": payload.description,
        "category": payload.category,
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _entry_from_node(dict(records[0]["x"]), user_id)


async def list_cashflows(session: AsyncSession, user_id: str, company_id: str, limit: int = 100) -> List[CashFlowEntry]:
    """Return the caller's most recent `limit` flows (total set upstream)."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASHFLOW]->(x:CashFlowEntry {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    flows = [_entry_from_node(dict(r["x"]), user_id) async for r in result]
    return flows[-limit:] if limit else flows


async def get_cash_position(session: AsyncSession, user_id: str, company_id: str) -> CashPosition:
    stored = await _get_stored_position(session, user_id, company_id)
    if stored is not None:
        return stored
    flows = await list_cashflows(session, user_id, company_id, limit=0)
    total = sum(f.amount if f.flow_type == CashFlowType.INFLOW else -f.amount for f in flows)
    return CashPosition(company_id=company_id, total_cash=total, available_cash=total)


async def _get_stored_position(session: AsyncSession, user_id: str, company_id: str) -> Optional[CashPosition]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_POSITION]->(x:CashPosition {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    records = [r async for r in result]
    if not records:
        return None
    return _position_from_node(dict(records[0]["x"]), user_id)


async def update_cash_position(
    session: AsyncSession, user_id: str, company_id: str, payload: CashPositionUpdate
) -> CashPosition:
    stored = await _get_stored_position(session, user_id, company_id)
    pos_id = stored.id if stored is not None else str(uuid.uuid4())
    if stored is not None:
        query = f"""
        MATCH (u:User {{id: $user_id}})-[:OWNS_POSITION]->(x:CashPosition {{company_id: $company_id, id: $id}})
        {BOOK_FILTER}
        SET x.total_cash = toFloat($total_cash),
            x.available_cash = toFloat($available_cash),
            x.restricted_cash = toFloat($restricted_cash),
            x.short_term_investments = toFloat($short_term_investments),
            x.liquidity_level = $liquidity_level,
            x.as_of = datetime($as_of)
        RETURN x
        """
    else:
        query = """
        MATCH (u:User {id: $user_id})
        CREATE (x:CashPosition {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            company_id: $company_id,
            total_cash: toFloat($total_cash),
            available_cash: toFloat($available_cash),
            restricted_cash: toFloat($restricted_cash),
            short_term_investments: toFloat($short_term_investments),
            liquidity_level: $liquidity_level,
            as_of: datetime($as_of),
            created_at: datetime($created_at)
        })
        CREATE (u)-[:OWNS_POSITION]->(x)
        RETURN x
        """
    # Liquidity is derived from 30-day outflow burn, computed server side.
    flows = await list_cashflows(session, user_id, company_id, limit=0)
    monthly_burn = abs(
        sum(f.amount for f in flows if f.flow_type == CashFlowType.OUTFLOW and f.date > _now() - timedelta(days=30))
    )
    liquidity = assess_liquidity(payload.available_cash, monthly_burn)
    params = {
        "id": pos_id,
        "user_id": user_id,
        "company_id": company_id,
        "total_cash": payload.total_cash,
        "available_cash": payload.available_cash,
        "restricted_cash": payload.restricted_cash,
        "short_term_investments": payload.short_term_investments,
        "liquidity_level": liquidity.value,
        "as_of": (payload.as_of or _now()).isoformat(),
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _position_from_node(dict(records[0]["x"]), user_id)


async def generate_forecast(session: AsyncSession, user_id: str, company_id: str, days: int = 30) -> CashFlowForecast:
    flows = await list_cashflows(session, user_id, company_id, limit=0)
    period_start = _now()
    period_end = period_start + timedelta(days=days)
    if not flows:
        return CashFlowForecast(
            company_id=company_id,
            period_start=period_start,
            period_end=period_end,
            projected_inflows=0,
            projected_outflows=0,
            net_cash_flow=0,
            ending_position=0,
            assumptions=["No historical data - forecast is zero-based"],
        )
    first_date = flows[0].date
    elapsed_days = max(1, (period_start - first_date).days)
    avg_daily_inflow = sum(f.amount for f in flows if f.flow_type == CashFlowType.INFLOW) / elapsed_days
    avg_daily_outflow = abs(sum(f.amount for f in flows if f.flow_type == CashFlowType.OUTFLOW)) / elapsed_days
    projected_in = avg_daily_inflow * days
    projected_out = avg_daily_outflow * days
    stored = await _get_stored_position(session, user_id, company_id)
    current = stored.available_cash if stored is not None else 0
    return CashFlowForecast(
        company_id=company_id,
        period_start=period_start,
        period_end=period_end,
        projected_inflows=round(projected_in, 2),
        projected_outflows=round(projected_out, 2),
        net_cash_flow=round(projected_in - projected_out, 2),
        ending_position=round(current + projected_in - projected_out, 2),
        assumptions=[
            f"Average daily inflow: {avg_daily_inflow:.2f}",
            f"Average daily outflow: {avg_daily_outflow:.2f}",
            f"Forecast period: {days} days",
        ],
    )
