"""
Fund Accounting Service CRUD Operations

Neo4j-backed persistence for funds and fund transactions. All records
are stamped with book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)`.

Funds are linked via :OWNS_FUND; transactions are owned by the user
(:OWNS_TRANSACTION) and attached to their fund via :HAS_TX, with the
fund's income/expenses/net_assets aggregates recomputed server side on
each recorded transaction.
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from fund_accounting_service.dependencies import book_id_var
from fund_accounting_service.models import (
    Fund,
    FundCreate,
    FundTransaction,
    FundTransactionCreate,
)
from neo4j import AsyncSession

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
    if isinstance(value, str):
        return datetime.fromisoformat(value)
    return datetime.fromisoformat(value.iso_format())


def _fund_from_node(n: Dict[str, Any], user_id: str) -> Fund:
    return Fund(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        fund_name=n["fund_name"],
        fund_type=n.get("fund_type", "general"),
        balance=float(n.get("balance", 0)),
        income=float(n.get("income", 0)),
        expenses=float(n.get("expenses", 0)),
        net_assets=float(n.get("net_assets", 0)),
        restrictions=n.get("restrictions", ""),
        manager=n.get("manager", ""),
        created_at=_dt(n.get("created_at")),
    )


def _tx_from_node(n: Dict[str, Any], user_id: str) -> FundTransaction:
    return FundTransaction(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        fund_id=n["fund_id"],
        description=n["description"],
        amount=float(n.get("amount", 0)),
        is_income=bool(n.get("is_income", False)),
        category=n.get("category", ""),
        date=_dt(n.get("date")),
    )


async def create_fund(session: AsyncSession, user_id: str, payload: FundCreate) -> Fund:
    fund = Fund(
        company_id=payload.company_id,
        fund_name=payload.fund_name,
        fund_type=payload.fund_type,
        balance=payload.balance,
        income=payload.income,
        expenses=payload.expenses,
        restrictions=payload.restrictions,
        manager=payload.manager,
    )
    # Original semantics: net_assets derived from balance + income - expenses
    fund.net_assets = fund.balance + fund.income - fund.expenses

    fund_id = str(uuid.uuid4())
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:Fund {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        fund_name: $fund_name,
        fund_type: $fund_type,
        balance: toFloat($balance),
        income: toFloat($income),
        expenses: toFloat($expenses),
        net_assets: toFloat($net_assets),
        restrictions: $restrictions,
        manager: $manager,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_FUND]->(x)
    RETURN x
    """
    params = {
        "id": fund_id,
        "user_id": user_id,
        "company_id": fund.company_id,
        "fund_name": fund.fund_name,
        "fund_type": fund.fund_type,
        "balance": fund.balance,
        "income": fund.income,
        "expenses": fund.expenses,
        "net_assets": fund.net_assets,
        "restrictions": fund.restrictions,
        "manager": fund.manager,
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _fund_from_node(dict(records[0]["x"]), user_id)


async def get_funds(session: AsyncSession, user_id: str, company_id: str) -> List[Fund]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_FUND]->(x:Fund {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    return [_fund_from_node(dict(r["x"]), user_id) async for r in result]


async def record_transaction(session: AsyncSession, user_id: str, payload: FundTransactionCreate) -> FundTransaction:
    """Record a transaction against one of the caller's own, Book-visible funds."""
    # Fund must belong to the caller and be visible in the current Book
    fund_query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_FUND]->(f:Fund {{id: $fund_id}})
    {BOOK_FILTER.replace("x.", "f.")}
    RETURN f
    """
    fund_result = await _run(session, fund_query, user_id=user_id, fund_id=payload.fund_id)
    fund_records = [r async for r in fund_result]
    if not fund_records:
        return None  # caller resolves to 404

    tx = FundTransaction(
        fund_id=payload.fund_id,
        description=payload.description,
        amount=payload.amount,
        is_income=payload.is_income,
        category=payload.category,
    )
    tx_id = str(uuid.uuid4())
    tx_query = """
    MATCH (u:User {id: $user_id})
    MATCH (f:Fund {id: $fund_id})
    CREATE (t:FundTransaction {
        id: $tx_id,
        user_id: $user_id,
        book_id: $book_id,
        fund_id: $fund_id,
        description: $description,
        amount: toFloat($amount),
        is_income: $is_income,
        category: $category,
        date: datetime($date)
    })
    CREATE (u)-[:OWNS_TRANSACTION]->(t)
    CREATE (f)-[:HAS_TX]->(t)
    RETURN t
    """
    tx_params = {
        "tx_id": tx_id,
        "user_id": user_id,
        "fund_id": tx.fund_id,
        "description": tx.description,
        "amount": tx.amount,
        "is_income": tx.is_income,
        "category": tx.category,
        "date": _now().isoformat(),
    }
    tx_result = await _run(session, tx_query, tx_params)
    tx_records = [r async for r in tx_result]
    stored = _tx_from_node(dict(tx_records[0]["t"]), user_id)

    # Recompute the fund's aggregates from all of its transactions
    all_tx_query = """
    MATCH (tx:FundTransaction {fund_id: $fund_id})
    RETURN tx
    """
    all_tx_result = await _run(session, all_tx_query, fund_id=tx.fund_id)
    all_txs = [_tx_from_node(dict(r["tx"]), user_id) async for r in all_tx_result]
    income = sum(t.amount for t in all_txs if t.is_income)
    expenses = sum(t.amount for t in all_txs if not t.is_income)

    fund_node = dict(fund_records[0]["f"])
    balance = float(fund_node.get("balance", 0) or 0)
    net_assets = balance + income - expenses
    update_query = """
    MATCH (f:Fund {id: $fund_id})
    SET f.income = toFloat($income),
        f.expenses = toFloat($expenses),
        f.net_assets = toFloat($net_assets)
    """
    await _run(session, update_query, fund_id=tx.fund_id, income=income, expenses=expenses, net_assets=net_assets)
    return stored


async def get_transactions(session: AsyncSession, user_id: str, fund_id: str) -> List[FundTransaction]:
    # Only list transactions when the fund is owned by the caller and Book-visible
    fund_query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_FUND]->(f:Fund {{id: $fund_id}})
    {BOOK_FILTER.replace("x.", "f.")}
    RETURN f
    """
    fund_result = await _run(session, fund_query, user_id=user_id, fund_id=fund_id)
    fund_records = [r async for r in fund_result]
    if not fund_records:
        return []

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_TRANSACTION]->(t:FundTransaction {{fund_id: $fund_id}})
    {BOOK_FILTER.replace("x.", "t.")}
    RETURN t
    ORDER BY t.date ASC
    """
    result = await _run(session, query, user_id=user_id, fund_id=fund_id)
    return [_tx_from_node(dict(r["t"]), user_id) async for r in result]
