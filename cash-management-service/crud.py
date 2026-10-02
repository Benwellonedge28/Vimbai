"""
Cash Management Service CRUD Operations

Neo4j-backed persistence for cash accounts, transfers, and liquidity
positions. All records are caller-owned (X-User-Id) and Book-gated
(X-Book-ID). Transfer balance arithmetic is computed in Python and written
back to the account nodes (the fake test harness cannot do node-prop
arithmetic in SET). Cross-scope reads/updates are 404, matching the
original "not found" semantics.
"""

import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

from cash_management_service.dependencies import book_id_var
from cash_management_service.exceptions import NotFoundError
from cash_management_service.models import CashAccount, CashTransfer, LiquidityPosition
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session: AsyncSession, query: str, params: Optional[Dict] = None, **kw):
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


def _account_from_node(n: Dict) -> CashAccount:
    return CashAccount(
        id=n["id"],
        account_name=n["account_name"],
        bank=n["bank"],
        account_number=n.get("account_number", ""),
        currency=n.get("currency", "USD"),
        balance=float(n.get("balance", 0.0)),
        min_balance=float(n.get("min_balance", 0.0)),
        type=n.get("type", "operating"),
        status=n.get("status", "active"),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _transfer_from_node(n: Dict) -> CashTransfer:
    return CashTransfer(
        id=n["id"],
        from_account_id=n["from_account_id"],
        to_account_id=n["to_account_id"],
        amount=float(n.get("amount", 0.0)),
        currency=n.get("currency", "USD"),
        transfer_date=_as_dt(n.get("transfer_date")) or _now(),
        status=n.get("status", "pending"),
        reference=n.get("reference", ""),
        notes=n.get("notes", ""),
    )


def _position_from_node(n: Dict) -> LiquidityPosition:
    return LiquidityPosition(
        id=n["id"],
        position_date=_as_dt(n.get("position_date")) or _now(),
        total_cash=float(n.get("total_cash", 0.0)),
        operating_cash=float(n.get("operating_cash", 0.0)),
        reserve_cash=float(n.get("reserve_cash", 0.0)),
        invested_cash=float(n.get("invested_cash", 0.0)),
        short_term_obligations=float(n.get("short_term_obligations", 0.0)),
        liquidity_ratio=float(n.get("liquidity_ratio", 0.0)),
    )


async def _list(session: AsyncSession, user_id: str, label: str, edge: str) -> List[Dict]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [dict(r["x"]) async for r in result]


async def _get_by_id(session: AsyncSession, user_id: str, label: str, edge: str, node_id: str) -> Optional[Dict]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    WHERE x.id = $node_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, node_id=node_id)
    record = await result.single()
    return dict(record["x"]) if record else None


async def set_balance(session: AsyncSession, user_id: str, node_id: str, balance: float) -> None:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_CASH_ACCOUNT]->(x:CashAccount)
    WHERE x.id = $node_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.balance = toFloat($balance)
    """
    await _run(session, query, {"balance": balance}, user_id=user_id, node_id=node_id)


# --- accounts ---


async def create_account(session: AsyncSession, user_id: str, account: CashAccount) -> CashAccount:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:CashAccount {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        account_name: $account_name,
        bank: $bank,
        account_number: $account_number,
        currency: $currency,
        balance: toFloat($balance),
        min_balance: toFloat($min_balance),
        type: $type,
        status: $status,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_CASH_ACCOUNT]->(x)
    RETURN x
    """
    result = await _run(
        session,
        query,
        {
            "id": account.id,
            "account_name": account.account_name,
            "bank": account.bank,
            "account_number": account.account_number,
            "currency": account.currency,
            "balance": account.balance,
            "min_balance": account.min_balance,
            "type": account.type,
            "status": account.status,
            "created_at": account.created_at.isoformat(),
        },
        user_id=user_id,
    )
    records = [r async for r in result]
    return _account_from_node(dict(records[0]["x"]))


async def list_accounts(session: AsyncSession, user_id: str, type: Optional[str] = None) -> List[CashAccount]:
    accounts = [_account_from_node(n) for n in await _list(session, user_id, "CashAccount", "OWNS_CASH_ACCOUNT")]
    if type:
        accounts = [a for a in accounts if a.type == type]
    return accounts


async def get_account(session: AsyncSession, user_id: str, account_id: str) -> CashAccount:
    node = await _get_by_id(session, user_id, "CashAccount", "OWNS_CASH_ACCOUNT", account_id)
    if not node:
        raise NotFoundError("Account not found")
    return _account_from_node(node)


# --- transfers ---


async def create_transfer(session: AsyncSession, user_id: str, transfer: CashTransfer) -> CashTransfer:
    """Persist the transfer record; balance movements are applied by the caller."""
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:CashTransfer {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        from_account_id: $from_account_id,
        to_account_id: $to_account_id,
        amount: toFloat($amount),
        currency: $currency,
        transfer_date: datetime($transfer_date),
        status: $status,
        reference: $reference,
        notes: $notes
    })
    CREATE (u)-[:OWNS_CASH_TRANSFER]->(x)
    RETURN x
    """
    result = await _run(
        session,
        query,
        {
            "id": transfer.id,
            "from_account_id": transfer.from_account_id,
            "to_account_id": transfer.to_account_id,
            "amount": transfer.amount,
            "currency": transfer.currency,
            "transfer_date": transfer.transfer_date.isoformat(),
            "status": transfer.status,
            "reference": transfer.reference,
            "notes": transfer.notes,
        },
        user_id=user_id,
    )
    records = [r async for r in result]
    return _transfer_from_node(dict(records[0]["x"]))


async def list_transfers(session: AsyncSession, user_id: str, limit: int = 50) -> List[CashTransfer]:
    transfers = [_transfer_from_node(n) for n in await _list(session, user_id, "CashTransfer", "OWNS_CASH_TRANSFER")]
    return transfers[-limit:]


# --- liquidity positions ---


async def save_position(session: AsyncSession, user_id: str, position: LiquidityPosition) -> LiquidityPosition:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:LiquidityPosition {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        position_date: datetime($position_date),
        total_cash: toFloat($total_cash),
        operating_cash: toFloat($operating_cash),
        reserve_cash: toFloat($reserve_cash),
        invested_cash: toFloat($invested_cash),
        short_term_obligations: toFloat($short_term_obligations),
        liquidity_ratio: toFloat($liquidity_ratio)
    })
    CREATE (u)-[:OWNS_LIQUIDITY_POSITION]->(x)
    RETURN x
    """
    result = await _run(
        session,
        query,
        {
            "id": position.id,
            "position_date": position.position_date.isoformat(),
            "total_cash": position.total_cash,
            "operating_cash": position.operating_cash,
            "reserve_cash": position.reserve_cash,
            "invested_cash": position.invested_cash,
            "short_term_obligations": position.short_term_obligations,
            "liquidity_ratio": position.liquidity_ratio,
        },
        user_id=user_id,
    )
    records = [r async for r in result]
    return _position_from_node(dict(records[0]["x"]))


async def list_positions(session: AsyncSession, user_id: str, limit: int = 30) -> List[LiquidityPosition]:
    positions = [
        _position_from_node(n) for n in await _list(session, user_id, "LiquidityPosition", "OWNS_LIQUIDITY_POSITION")
    ]
    return positions[-limit:]
