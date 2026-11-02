"""
Investment Monitoring Service CRUD Operations

Portfolios, holdings and performance snapshots are nodes, Book-gated, so one
caller's portfolio valuations are invisible to another.

QUIRKS preserved:
* a holding is only added to a portfolio that exists, but the holdings listing for
  an unknown portfolio is an empty list rather than a 404;
* recording performance checks the holdings, not the portfolio: an empty or
  unknown portfolio answers the same 404 "No holdings found for portfolio";
* the return percentage is zero when there is nothing invested;
* the performance listing returns the most recent entries, cut to the limit from
  the end of the chronological list.
"""

import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

from investment_monitoring_service.dependencies import book_id_var
from investment_monitoring_service.exceptions import NotFoundError
from investment_monitoring_service.models import InvestmentHolding, PerformanceMetric, Portfolio
from neo4j import AsyncSession

PORTFOLIO_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


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
        if iso in (None, "None"):
            return None
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _portfolio_from_node(n: Dict, user_id: str) -> Portfolio:
    return Portfolio(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        name=n.get("name") or "",
        description=n.get("description") or "",
        target_return=float(n.get("target_return") or 0),
        risk_tolerance=n.get("risk_tolerance") or "moderate",
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _holding_from_node(n: Dict, user_id: str) -> InvestmentHolding:
    return InvestmentHolding(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        portfolio_id=n.get("portfolio_id") or "",
        instrument_name=n.get("instrument_name") or "",
        instrument_type=n.get("instrument_type") or "",
        quantity=float(n.get("quantity") or 0),
        purchase_price=float(n.get("purchase_price") or 0),
        current_price=float(n.get("current_price") or 0),
        currency=n.get("currency") or "USD",
        sector=n.get("sector") or "",
        updated_at=_as_dt(n.get("updated_at")) or _now(),
    )


def _metric_from_node(n: Dict, user_id: str) -> PerformanceMetric:
    return PerformanceMetric(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        portfolio_id=n.get("portfolio_id") or "",
        metric_date=_as_dt(n.get("metric_date")) or _now(),
        total_value=float(n.get("total_value") or 0),
        total_cost=float(n.get("total_cost") or 0),
        unrealized_gain=float(n.get("unrealized_gain") or 0),
        unrealized_gain_pct=float(n.get("unrealized_gain_pct") or 0),
        daily_return=float(n.get("daily_return") or 0),
        cumulative_return=float(n.get("cumulative_return") or 0),
    )


async def get_portfolio(session: AsyncSession, user_id: str, portfolio_id: str) -> Portfolio:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PORTFOLIO]->(x:Portfolio {{id: $portfolio_id}})
    {PORTFOLIO_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, portfolio_id=portfolio_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Portfolio not found")
    return _portfolio_from_node(dict(records[0]["x"]), user_id)


async def create_portfolio(
    session: AsyncSession,
    user_id: str,
    *,
    name: str,
    description: str = "",
    target_return: float = 0.0,
    risk_tolerance: str = "moderate",
) -> Portfolio:
    portfolio = Portfolio(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        name=name,
        description=description,
        target_return=target_return,
        risk_tolerance=risk_tolerance,
    )
    await _run(
        session,
        """
        MATCH (u:User {id: $user_id})
        CREATE (x:Portfolio {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            name: $name,
            description: $description,
            target_return: toFloat($target_return),
            risk_tolerance: $risk_tolerance,
            created_at: datetime($created_at)
        })
        CREATE (u)-[:OWNS_PORTFOLIO]->(x)
        RETURN x
        """,
        {
            "id": portfolio.id,
            "user_id": user_id,
            "name": name,
            "description": description,
            "target_return": target_return,
            "risk_tolerance": risk_tolerance,
            "created_at": portfolio.created_at.isoformat(),
        },
    )
    return portfolio


async def list_portfolios(session: AsyncSession, user_id: str) -> List[Portfolio]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PORTFOLIO]->(x:Portfolio)
    {PORTFOLIO_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    return [_portfolio_from_node(dict(r["x"]), user_id) async for r in result]


async def add_holding(
    session: AsyncSession,
    user_id: str,
    portfolio_id: str,
    *,
    instrument_name: str,
    instrument_type: str,
    quantity: float,
    purchase_price: float,
    current_price: float,
    currency: str = "USD",
    sector: str = "",
) -> InvestmentHolding:
    await get_portfolio(session, user_id, portfolio_id)

    holding = InvestmentHolding(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        portfolio_id=portfolio_id,
        instrument_name=instrument_name,
        instrument_type=instrument_type,
        quantity=quantity,
        purchase_price=purchase_price,
        current_price=current_price,
        currency=currency,
        sector=sector,
    )
    await _run(
        session,
        """
        MATCH (u:User {id: $user_id})
        CREATE (x:InvestmentHolding {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            portfolio_id: $portfolio_id,
            instrument_name: $instrument_name,
            instrument_type: $instrument_type,
            quantity: toFloat($quantity),
            purchase_price: toFloat($purchase_price),
            current_price: toFloat($current_price),
            currency: $currency,
            sector: $sector,
            updated_at: datetime($updated_at)
        })
        CREATE (u)-[:OWNS_HOLDING]->(x)
        RETURN x
        """,
        {
            "id": holding.id,
            "user_id": user_id,
            "portfolio_id": portfolio_id,
            "instrument_name": instrument_name,
            "instrument_type": instrument_type,
            "quantity": quantity,
            "purchase_price": purchase_price,
            "current_price": current_price,
            "currency": currency,
            "sector": sector,
            "updated_at": holding.updated_at.isoformat(),
        },
    )
    return holding


async def _holdings_for(session: AsyncSession, user_id: str, portfolio_id: str) -> List[InvestmentHolding]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_HOLDING]->(x:InvestmentHolding {{portfolio_id: $portfolio_id}})
    {PORTFOLIO_FILTER}
    RETURN x
    ORDER BY x.instrument_name ASC, x.id ASC
    """
    result = await _run(session, query, user_id=user_id, portfolio_id=portfolio_id)
    return [_holding_from_node(dict(r["x"]), user_id) async for r in result]


async def list_holdings(session: AsyncSession, user_id: str, portfolio_id: str) -> List[InvestmentHolding]:
    """An unknown or foreign portfolio simply has no holdings, as before."""
    return await _holdings_for(session, user_id, portfolio_id)


async def record_performance(session: AsyncSession, user_id: str, portfolio_id: str) -> PerformanceMetric:
    portfolio_holdings = await _holdings_for(session, user_id, portfolio_id)
    if not portfolio_holdings:
        raise NotFoundError("No holdings found for portfolio")

    total_value = sum(h.current_price * h.quantity for h in portfolio_holdings)
    total_cost = sum(h.purchase_price * h.quantity for h in portfolio_holdings)
    unrealized_gain = total_value - total_cost
    unrealized_gain_pct = (unrealized_gain / total_cost * 100) if total_cost > 0 else 0.0

    metric = PerformanceMetric(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        portfolio_id=portfolio_id,
        metric_date=_now(),
        total_value=total_value,
        total_cost=total_cost,
        unrealized_gain=unrealized_gain,
        unrealized_gain_pct=unrealized_gain_pct,
    )
    await _run(
        session,
        """
        MATCH (u:User {id: $user_id})
        CREATE (x:PerformanceMetric {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            portfolio_id: $portfolio_id,
            metric_date: datetime($metric_date),
            total_value: toFloat($total_value),
            total_cost: toFloat($total_cost),
            unrealized_gain: toFloat($unrealized_gain),
            unrealized_gain_pct: toFloat($unrealized_gain_pct)
        })
        CREATE (u)-[:OWNS_PERFORMANCE_METRIC]->(x)
        RETURN x
        """,
        {
            "id": metric.id,
            "user_id": user_id,
            "portfolio_id": portfolio_id,
            "metric_date": metric.metric_date.isoformat(),
            "total_value": total_value,
            "total_cost": total_cost,
            "unrealized_gain": unrealized_gain,
            "unrealized_gain_pct": unrealized_gain_pct,
        },
    )
    return metric


async def list_performance(
    session: AsyncSession, user_id: str, portfolio_id: str, limit: int = 30
) -> List[PerformanceMetric]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PERFORMANCE_METRIC]->(x:PerformanceMetric {{portfolio_id: $portfolio_id}})
    {PORTFOLIO_FILTER}
    RETURN x
    ORDER BY x.metric_date ASC
    """
    result = await _run(session, query, user_id=user_id, portfolio_id=portfolio_id)
    metrics = [_metric_from_node(dict(r["x"]), user_id) async for r in result]
    return metrics[-limit:]


async def update_price(session: AsyncSession, user_id: str, holding_id: str, new_price: float) -> Dict[str, object]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_HOLDING]->(x:InvestmentHolding {{id: $holding_id}})
    {PORTFOLIO_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, holding_id=holding_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Holding not found")

    await _run(
        session,
        """
        MATCH (x:InvestmentHolding {id: $holding_id})
        SET x.current_price = toFloat($new_price), x.updated_at = datetime($updated_at)
        """,
        {"holding_id": holding_id, "new_price": new_price, "updated_at": _now().isoformat()},
    )
    return {"holding_id": holding_id, "new_price": new_price}
