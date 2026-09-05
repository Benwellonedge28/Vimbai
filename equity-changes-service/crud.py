"""
Equity Changes Service CRUD Operations

Neo4j-backed persistence for equity transactions and statements. All
records are stamped with book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)`.

Transactions are independent records (linked via :OWNS_TRANSACTION).
Statements embed their transaction list as a JSON prop (the original API
computes statement totals from the transactions supplied in the payload,
not from stored ones); statements are immutable records - corrections
are new statements, matching the ledger-style design.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from equity_changes_service.dependencies import book_id_var
from equity_changes_service.models import (
    EquityStatement,
    EquityStatementCreate,
    EquityTransaction,
    EquityTransactionCreate,
    EquityTransactionType,
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


def _json(value) -> list:
    if value is None:
        return []
    if isinstance(value, str):
        return json.loads(value)
    return value


def _tx_from_node(n: Dict[str, Any], user_id: str) -> EquityTransaction:
    return EquityTransaction(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        transaction_type=n["transaction_type"],
        shareholder=n.get("shareholder", ""),
        shares=int(n.get("shares", 0)),
        price_per_share=float(n.get("price_per_share", 0)),
        amount=float(n.get("amount", 0)),
        description=n.get("description", ""),
        date=_dt(n.get("date")),
        created_at=_dt(n.get("created_at")),
    )


def _stmt_from_node(n: Dict[str, Any], user_id: str) -> EquityStatement:
    return EquityStatement(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        period=n["period"],
        beginning_equity=float(n["beginning_equity"]),
        share_issuances=float(n["share_issuances"]),
        share_buybacks=float(n["share_buybacks"]),
        dividends_paid=float(n["dividends_paid"]),
        retained_earnings_change=float(n["retained_earnings_change"]),
        other_changes=float(n["other_changes"]),
        ending_equity=float(n["ending_equity"]),
        transactions=[EquityTransaction(**t) for t in _json(n.get("transactions"))],
        created_at=_dt(n.get("created_at")),
    )


async def create_transaction(
    session: AsyncSession, user_id: str, payload: EquityTransactionCreate
) -> EquityTransaction:
    tx = EquityTransaction(company_id=payload.company_id, transaction_type=payload.transaction_type)
    tx.shareholder = payload.shareholder
    tx.shares = payload.shares
    tx.price_per_share = payload.price_per_share
    tx.description = payload.description
    tx.date = payload.date or _now()
    # Original semantics: derive amount from shares * price when unset
    tx.amount = tx.shares * tx.price_per_share if payload.amount == 0 and tx.shares > 0 else payload.amount

    tx_id = str(uuid.uuid4())
    now = _now()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:EquityTransaction {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        transaction_type: $transaction_type,
        shareholder: $shareholder,
        shares: toInteger($shares),
        price_per_share: toFloat($price_per_share),
        amount: toFloat($amount),
        description: $description,
        date: datetime($date),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_TRANSACTION]->(x)
    RETURN x
    """
    params = {
        "id": tx_id,
        "user_id": user_id,
        "company_id": tx.company_id,
        "transaction_type": tx.transaction_type.value,
        "shareholder": tx.shareholder,
        "shares": tx.shares,
        "price_per_share": tx.price_per_share,
        "amount": tx.amount,
        "description": tx.description,
        "date": tx.date.isoformat(),
        "created_at": now.isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _tx_from_node(dict(records[0]["x"]), user_id)


async def get_transactions(
    session: AsyncSession, user_id: str, company_id: str, tx_type: Optional[str] = None
) -> List[EquityTransaction]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_TRANSACTION]->(x:EquityTransaction {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    txs = [_tx_from_node(dict(r["x"]), user_id) async for r in result]
    if tx_type:
        txs = [t for t in txs if t.transaction_type.value == tx_type]
    return txs


async def generate_statement(session: AsyncSession, user_id: str, payload: EquityStatementCreate) -> EquityStatement:
    stmt = EquityStatement(
        company_id=payload.company_id,
        period=payload.period,
        beginning_equity=payload.beginning_equity,
        transactions=[EquityTransaction(**t.model_dump(exclude_none=True)) for t in payload.transactions],
    )
    # Original semantics: derive amount when unset, then roll up by type
    for t in stmt.transactions:
        t.date = t.date or _now()
        t.amount = t.shares * t.price_per_share if t.amount == 0 and t.shares > 0 else t.amount
    stmt.share_issuances = sum(
        t.amount for t in stmt.transactions if t.transaction_type == EquityTransactionType.ISSUANCE
    )
    stmt.share_buybacks = sum(
        t.amount for t in stmt.transactions if t.transaction_type == EquityTransactionType.BUYBACK
    )
    stmt.dividends_paid = sum(
        t.amount for t in stmt.transactions if t.transaction_type == EquityTransactionType.DIVIDEND
    )
    stmt.retained_earnings_change = sum(
        t.amount for t in stmt.transactions if t.transaction_type == EquityTransactionType.RETAINED
    )
    stmt.other_changes = sum(
        t.amount
        for t in stmt.transactions
        if t.transaction_type in (EquityTransactionType.SPLIT, EquityTransactionType.TRANSFER)
    )
    stmt.ending_equity = (
        stmt.beginning_equity
        + stmt.share_issuances
        - stmt.share_buybacks
        - stmt.dividends_paid
        + stmt.retained_earnings_change
        + stmt.other_changes
    )

    stmt_id = str(uuid.uuid4())
    now = _now()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:EquityStatement {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        period: $period,
        beginning_equity: toFloat($beginning_equity),
        share_issuances: toFloat($share_issuances),
        share_buybacks: toFloat($share_buybacks),
        dividends_paid: toFloat($dividends_paid),
        retained_earnings_change: toFloat($retained_earnings_change),
        other_changes: toFloat($other_changes),
        ending_equity: toFloat($ending_equity),
        transactions: $transactions,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_STATEMENT]->(x)
    RETURN x
    """
    params = {
        "id": stmt_id,
        "user_id": user_id,
        "company_id": stmt.company_id,
        "period": stmt.period,
        "beginning_equity": stmt.beginning_equity,
        "share_issuances": stmt.share_issuances,
        "share_buybacks": stmt.share_buybacks,
        "dividends_paid": stmt.dividends_paid,
        "retained_earnings_change": stmt.retained_earnings_change,
        "other_changes": stmt.other_changes,
        "ending_equity": stmt.ending_equity,
        "transactions": json.dumps([t.model_dump(mode="json") for t in stmt.transactions]),
        "created_at": now.isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _stmt_from_node(dict(records[0]["x"]), user_id)


async def get_statements(session: AsyncSession, user_id: str, company_id: str) -> List[EquityStatement]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_STATEMENT]->(x:EquityStatement {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    return [_stmt_from_node(dict(r["x"]), user_id) async for r in result]
