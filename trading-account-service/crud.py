"""
Trading Account Service CRUD Operations

:TradingAccount via :OWNS_TRADING_ACCOUNT, Book-gated, so one caller's generated
accounts are invisible to another.

QUIRKS preserved: the margin is zero rather than an error when net sales come out
at zero or below, and the margin is rounded to two places while the gross profit
itself is kept at full precision.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from trading_account_service.dependencies import book_id_var
from trading_account_service.exceptions import NotFoundError
from trading_account_service.models import TradingAccountRequest, TradingAccountResult

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
        if iso in (None, "None"):
            return None
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _from_node(n: Dict[str, Any], user_id: str) -> TradingAccountResult:
    return TradingAccountResult(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n.get("company_id") or "",
        period=n.get("period") or "",
        net_sales=float(n.get("net_sales") or 0),
        cost_of_goods_sold=float(n.get("cost_of_goods_sold") or 0),
        gross_profit=float(n.get("gross_profit") or 0),
        gross_profit_pct=float(n.get("gross_profit_pct") or 0),
        details=json.loads(n.get("details_json") or "{}"),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def compute(request: TradingAccountRequest) -> Dict[str, Any]:
    """The trading account arithmetic, unchanged from the original."""
    net_sales = request.sales - request.sales_returns
    cost_of_goods_sold = (
        request.opening_stock
        + request.purchases
        + request.carriage_inward
        + request.direct_wages
        + request.factory_overhead
        - request.closing_stock
    )
    gross_profit = net_sales - cost_of_goods_sold
    gross_profit_pct = (gross_profit / net_sales * 100) if net_sales else 0.0

    return {
        "net_sales": net_sales,
        "cost_of_goods_sold": cost_of_goods_sold,
        "gross_profit": gross_profit,
        "gross_profit_pct": round(gross_profit_pct, 2),
        "details": {
            "opening_stock": request.opening_stock,
            "purchases": request.purchases,
            "carriage_inward": request.carriage_inward,
            "closing_stock": request.closing_stock,
            "sales": request.sales,
            "sales_returns": request.sales_returns,
            "direct_wages": request.direct_wages,
            "factory_overhead": request.factory_overhead,
        },
    }


async def generate(session: AsyncSession, user_id: str, request: TradingAccountRequest) -> TradingAccountResult:
    figures = compute(request)
    record = TradingAccountResult(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        company_id=request.company_id,
        period=request.period,
        net_sales=figures["net_sales"],
        cost_of_goods_sold=figures["cost_of_goods_sold"],
        gross_profit=figures["gross_profit"],
        gross_profit_pct=figures["gross_profit_pct"],
        details=figures["details"],
        created_at=_now(),
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:TradingAccount {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        period: $period,
        net_sales: toFloat($net_sales),
        cost_of_goods_sold: toFloat($cost),
        gross_profit: toFloat($gross),
        gross_profit_pct: toFloat($percent),
        details_json: $details_json,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_TRADING_ACCOUNT]->(x)
    RETURN x
    """
    await _run(
        session,
        query,
        {
            "id": record.id,
            "user_id": user_id,
            "company_id": record.company_id,
            "period": record.period,
            "net_sales": record.net_sales,
            "cost": record.cost_of_goods_sold,
            "gross": record.gross_profit,
            "percent": record.gross_profit_pct,
            "details_json": json.dumps(record.details),
            "created_at": record.created_at.isoformat(),
        },
    )
    return record


async def list_accounts(
    session: AsyncSession, user_id: str, period: Optional[str] = None
) -> List[TradingAccountResult]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_TRADING_ACCOUNT]->(x:TradingAccount)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    accounts = [_from_node(dict(r["x"]), user_id) async for r in result]
    if period:
        accounts = [a for a in accounts if a.period == period]
    return accounts


async def get_account(session: AsyncSession, user_id: str, account_id: str) -> TradingAccountResult:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_TRADING_ACCOUNT]->(x:TradingAccount {{id: $account_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, account_id=account_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Trading account not found")
    return _from_node(dict(records[0]["x"]), user_id)
