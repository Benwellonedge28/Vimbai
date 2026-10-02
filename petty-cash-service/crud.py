"""
Petty Cash Service CRUD Operations

Neo4j-backed persistence for funds, transactions, vouchers, and
replenishments. Every record is stamped with user_id + book_id; every read
applies caller ownership plus the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)`. Cross-scope access is
404 (no existence leak).
"""

import json
import uuid
from datetime import datetime, timezone
from decimal import Decimal
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from petty_cash_service.dependencies import book_id_var
from petty_cash_service.exceptions import NotFoundError, ValidationError
from petty_cash_service.models import (
    PaymentCategory,
    PettyCashFund,
    PettyCashReplenishment,
    PettyCashStatus,
    PettyCashSummary,
    PettyCashTransaction,
    PettyCashVoucher,
    ReimbursementStatus,
    TransactionType,
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


def _dec(value, default="0") -> Decimal:
    """Exact money hydration: decimals are stored as strings."""
    if value is None:
        return Decimal(default)
    return value if isinstance(value, Decimal) else Decimal(str(value))


def _status(value, enum_cls):
    if isinstance(value, enum_cls):
        return value
    return enum_cls(value)


# --- hydration ---


def _fund_from_node(n: Dict[str, Any]) -> PettyCashFund:
    return PettyCashFund(
        id=n["id"],
        book_id=n.get("book_id"),
        fund_code=n["fund_code"],
        fund_name=n["fund_name"],
        custodian_id=n["custodian_id"],
        custodian_name=n["custodian_name"],
        location=n.get("location", ""),
        maximum_balance=_dec(n.get("maximum_balance_str")),
        minimum_balance=_dec(n.get("minimum_balance_str")),
        replenishment_threshold=_dec(n.get("replenishment_threshold_str")),
        replenishment_amount=_dec(n.get("replenishment_amount_str")),
        status=_status(n.get("status", "active"), PettyCashStatus),
        account_code=n.get("account_code", ""),
        created_at=_as_dt(n.get("created_at")) or _now(),
        updated_at=_as_dt(n.get("updated_at")) or _now(),
    )


def _tx_from_node(n: Dict[str, Any]) -> PettyCashTransaction:
    return PettyCashTransaction(
        id=n["id"],
        book_id=n.get("book_id"),
        fund_id=n["fund_id"],
        transaction_type=_status(n["transaction_type"], TransactionType),
        amount=_dec(n.get("amount_str")),
        date=_as_dt(n.get("date")) or _now(),
        description=n.get("description", ""),
        category=_status(n.get("category", "miscellaneous"), PaymentCategory),
        recipient_name=n.get("recipient_name"),
        recipient_id=n.get("recipient_id"),
        reference_number=n.get("reference_number", ""),
        voucher_number=n.get("voucher_number", ""),
        approved_by=n.get("approved_by"),
        entered_by=n.get("entered_by", ""),
        receipt_attachment=n.get("receipt_attachment"),
        notes=n.get("notes"),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _voucher_from_node(n: Dict[str, Any]) -> PettyCashVoucher:
    return PettyCashVoucher(
        id=n["id"],
        book_id=n.get("book_id"),
        fund_id=n["fund_id"],
        voucher_number=n["voucher_number"],
        date=_as_dt(n.get("date")) or _now(),
        payee=n.get("payee", ""),
        amount=_dec(n.get("amount_str")),
        description=n.get("description", ""),
        category=_status(n.get("category", "miscellaneous"), PaymentCategory),
        approved_by=n.get("approved_by"),
        receipt_attached=bool(n.get("receipt_attached", False)),
        status=n.get("status", "pending"),
        entered_by=n.get("entered_by", ""),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _repl_from_node(n: Dict[str, Any]) -> PettyCashReplenishment:
    return PettyCashReplenishment(
        id=n["id"],
        book_id=n.get("book_id"),
        fund_id=n["fund_id"],
        amount=_dec(n.get("amount_str")),
        request_date=_as_dt(n.get("request_date")) or _now(),
        requested_by=n.get("requested_by", ""),
        approved_by=n.get("approved_by"),
        approved_date=_as_dt(n.get("approved_date")),
        status=_status(n.get("status", "pending"), ReimbursementStatus),
        transactions_included=json.loads(n.get("transactions_included_json") or "[]"),
        total_cash_disbursed=_dec(n.get("total_cash_disbursed_str")),
        bank_reference=n.get("bank_reference"),
        notes=n.get("notes"),
    )


# --- generic reads ---


async def _list_nodes(session: AsyncSession, user_id: str, label: str, edge: str) -> List[Dict[str, Any]]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [dict(r["x"]) async for r in result]


async def _get_node(session: AsyncSession, user_id: str, label: str, edge: str, node_id: str) -> Dict[str, Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    WHERE x.id = $node_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, node_id=node_id)
    record = await result.single()
    if not record:
        return None
    return dict(record["x"])


async def _set_node_props(
    session: AsyncSession, user_id: str, label: str, edge: str, node_id: str, set_lines: str, params: Dict[str, Any]
):
    """Generic Book-gated SET on a caller-owned node."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    WHERE x.id = $node_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET {set_lines}
    """
    merged = dict(params)
    merged.update({"user_id": user_id, "node_id": node_id})
    result = await _run(session, query, merged)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Record not found")
    return dict(records[0]["x"])


# --- funds ---


async def create_fund(session: AsyncSession, user_id: str, fund: PettyCashFund) -> PettyCashFund:
    fund.id = str(uuid.uuid4())
    fund.created_at = _now()
    fund.updated_at = _now()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:PettyCashFund {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        fund_code: $fund_code,
        fund_name: $fund_name,
        custodian_id: $custodian_id,
        custodian_name: $custodian_name,
        location: $location,
        maximum_balance_str: $maximum_balance_str,
        minimum_balance_str: $minimum_balance_str,
        replenishment_threshold_str: $replenishment_threshold_str,
        replenishment_amount_str: $replenishment_amount_str,
        status: $status,
        account_code: $account_code,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_FUND]->(x)
    RETURN x
    """
    params = {
        "id": fund.id,
        "fund_code": fund.fund_code,
        "fund_name": fund.fund_name,
        "custodian_id": fund.custodian_id,
        "custodian_name": fund.custodian_name,
        "location": fund.location,
        "maximum_balance_str": str(fund.maximum_balance),
        "minimum_balance_str": str(fund.minimum_balance),
        "replenishment_threshold_str": str(fund.replenishment_threshold),
        "replenishment_amount_str": str(fund.replenishment_amount),
        "status": fund.status.value,
        "account_code": fund.account_code,
        "created_at": fund.created_at.isoformat(),
        "updated_at": fund.updated_at.isoformat(),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [r async for r in result]
    return _fund_from_node(dict(records[0]["x"]))


async def list_funds(
    session: AsyncSession, user_id: str, status: Optional[str] = None, custodian_id: Optional[str] = None
) -> List[PettyCashFund]:
    nodes = await _list_nodes(session, user_id, "PettyCashFund", "OWNS_FUND")
    funds = [_fund_from_node(n) for n in nodes]
    if status is not None:
        funds = [f for f in funds if f.status == status]
    if custodian_id:
        funds = [f for f in funds if f.custodian_id == custodian_id]
    return funds


async def get_fund(session: AsyncSession, user_id: str, fund_id: str) -> PettyCashFund:
    node = await _get_node(session, user_id, "PettyCashFund", "OWNS_FUND", fund_id)
    if not node:
        raise NotFoundError("Fund not found")
    return _fund_from_node(node)


async def update_fund(session: AsyncSession, user_id: str, fund_id: str, fund: PettyCashFund) -> PettyCashFund:
    await get_fund(session, user_id, fund_id)  # ownership + Book gate
    fund.id = fund_id
    fund.updated_at = _now()
    set_lines = """
        x.fund_code = $fund_code,
        x.fund_name = $fund_name,
        x.custodian_id = $custodian_id,
        x.custodian_name = $custodian_name,
        x.location = $location,
        x.maximum_balance_str = $maximum_balance_str,
        x.minimum_balance_str = $minimum_balance_str,
        x.replenishment_threshold_str = $replenishment_threshold_str,
        x.replenishment_amount_str = $replenishment_amount_str,
        x.status = $status,
        x.account_code = $account_code,
        x.updated_at = datetime($updated_at)
    """
    params = {
        "fund_code": fund.fund_code,
        "fund_name": fund.fund_name,
        "custodian_id": fund.custodian_id,
        "custodian_name": fund.custodian_name,
        "location": fund.location,
        "maximum_balance_str": str(fund.maximum_balance),
        "minimum_balance_str": str(fund.minimum_balance),
        "replenishment_threshold_str": str(fund.replenishment_threshold),
        "replenishment_amount_str": str(fund.replenishment_amount),
        "status": fund.status.value,
        "account_code": fund.account_code,
        "updated_at": fund.updated_at.isoformat(),
    }
    node = await _set_node_props(session, user_id, "PettyCashFund", "OWNS_FUND", fund_id, set_lines, params)
    return _fund_from_node(node)


async def close_fund(session: AsyncSession, user_id: str, fund_id: str) -> None:
    await get_fund(session, user_id, fund_id)
    set_lines = "x.status = $status,\n        x.updated_at = datetime($updated_at)"
    await _set_node_props(
        session,
        user_id,
        "PettyCashFund",
        "OWNS_FUND",
        fund_id,
        set_lines,
        {"status": PettyCashStatus.CLOSED.value, "updated_at": _now().isoformat()},
    )


# --- transactions ---


async def create_transaction(
    session: AsyncSession, user_id: str, tx: PettyCashTransaction, check_balance: bool = True
) -> PettyCashTransaction:
    fund = await get_fund(session, user_id, tx.fund_id)  # 404 if not caller's

    # Balance check for payments (original semantics: respect fund minimum).
    # Voucher-initiated payments skip the check (original behavior).
    if check_balance and tx.transaction_type == TransactionType.PAYMENT:
        balance_info = await fund_balance(session, user_id, tx.fund_id)
        current_balance = Decimal(balance_info["current_balance"])
        if current_balance - tx.amount < fund.minimum_balance:
            raise ValidationError(
                f"Insufficient balance. Minimum balance is {fund.minimum_balance}", code="INSUFFICIENT_BALANCE"
            )

    tx.id = str(uuid.uuid4())
    tx.created_at = _now()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:PettyCashTransaction {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        fund_id: $fund_id,
        transaction_type: $transaction_type,
        amount_str: $amount_str,
        date: datetime($date),
        description: $description,
        category: $category,
        recipient_name: $recipient_name,
        recipient_id: $recipient_id,
        reference_number: $reference_number,
        voucher_number: $voucher_number,
        approved_by: $approved_by,
        entered_by: $entered_by,
        receipt_attachment: $receipt_attachment,
        notes: $notes,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_TX]->(x)
    RETURN x
    """
    params = {
        "id": tx.id,
        "fund_id": tx.fund_id,
        "transaction_type": tx.transaction_type.value,
        "amount_str": str(tx.amount),
        "date": tx.date.isoformat(),
        "description": tx.description,
        "category": tx.category.value,
        "recipient_name": tx.recipient_name,
        "recipient_id": tx.recipient_id,
        "reference_number": tx.reference_number,
        "voucher_number": tx.voucher_number,
        "approved_by": tx.approved_by,
        "entered_by": tx.entered_by,
        "receipt_attachment": tx.receipt_attachment,
        "notes": tx.notes,
        "created_at": tx.created_at.isoformat(),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [r async for r in result]
    return _tx_from_node(dict(records[0]["x"]))


async def list_transactions(
    session: AsyncSession,
    user_id: str,
    fund_id: Optional[str] = None,
    transaction_type: Optional[TransactionType] = None,
    category: Optional[PaymentCategory] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100,
) -> List[PettyCashTransaction]:
    nodes = await _list_nodes(session, user_id, "PettyCashTransaction", "OWNS_TX")
    txs = [_tx_from_node(n) for n in nodes]
    if fund_id:
        txs = [t for t in txs if t.fund_id == fund_id]
    if transaction_type is not None:
        txs = [t for t in txs if t.transaction_type == transaction_type]
    if category is not None:
        txs = [t for t in txs if t.category == category]
    if start_date:
        txs = [t for t in txs if t.date >= start_date]
    if end_date:
        txs = [t for t in txs if t.date <= end_date]
    txs.sort(key=lambda t: t.date, reverse=True)
    return txs[:limit]


async def get_transaction(session: AsyncSession, user_id: str, tx_id: str) -> PettyCashTransaction:
    node = await _get_node(session, user_id, "PettyCashTransaction", "OWNS_TX", tx_id)
    if not node:
        raise NotFoundError("Transaction not found")
    return _tx_from_node(node)


async def fund_balance(session: AsyncSession, user_id: str, fund_id: str) -> Dict[str, str]:
    """Current balance for a caller-owned fund (original semantics)."""
    await get_fund(session, user_id, fund_id)
    txs = [t for t in await list_transactions(session, user_id, fund_id=fund_id, limit=10**9)]
    total_receipts = sum(
        t.amount
        for t in txs
        if t.transaction_type in [TransactionType.RECEIPT, TransactionType.REPLENISHMENT, TransactionType.INITIAL_FUND]
    )
    total_payments = sum(t.amount for t in txs if t.transaction_type == TransactionType.PAYMENT)
    return {
        "fund_id": fund_id,
        "total_receipts": str(total_receipts),
        "total_payments": str(total_payments),
        "current_balance": str(total_receipts - total_payments),
    }


async def fund_summary(session: AsyncSession, user_id: str, fund_id: str) -> PettyCashSummary:
    """Fund summary (original semantics, computed over caller's transactions)."""
    fund = await get_fund(session, user_id, fund_id)
    txs = [t for t in await list_transactions(session, user_id, fund_id=fund_id, limit=10**9)]
    balance_info = await fund_balance(session, user_id, fund_id)

    pending_vouchers = sum(1 for t in txs if getattr(t, "status", None) == "pending")

    current_balance = Decimal(balance_info["current_balance"])
    replenishment_needed = current_balance < fund.replenishment_threshold
    replenishments = [t for t in txs if t.transaction_type == TransactionType.REPLENISHMENT]
    last_repl = max((t.date for t in replenishments), default=None)

    return PettyCashSummary(
        fund_id=fund_id,
        fund_name=fund.fund_name,
        opening_balance=Decimal("0"),
        total_receipts=Decimal(balance_info["total_receipts"]),
        total_payments=Decimal(balance_info["total_payments"]),
        closing_balance=current_balance,
        outstanding_vouchers=pending_vouchers,
        available_cash=current_balance,
        replenishment_needed=replenishment_needed,
        last_replenishment_date=last_repl,
    )


# --- vouchers ---


async def create_voucher(session: AsyncSession, user_id: str, voucher: PettyCashVoucher) -> Dict[str, Any]:
    """Create a voucher and its corresponding PAYMENT transaction (original semantics)."""
    await get_fund(session, user_id, voucher.fund_id)  # 404 if not caller's
    voucher.id = str(uuid.uuid4())
    voucher.created_at = _now()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:PettyCashVoucher {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        fund_id: $fund_id,
        voucher_number: $voucher_number,
        date: datetime($date),
        payee: $payee,
        amount_str: $amount_str,
        description: $description,
        category: $category,
        approved_by: $approved_by,
        receipt_attached: $receipt_attached,
        status: $status,
        entered_by: $entered_by,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_VOUCHER]->(x)
    RETURN x
    """
    params = {
        "id": voucher.id,
        "fund_id": voucher.fund_id,
        "voucher_number": voucher.voucher_number,
        "date": voucher.date.isoformat(),
        "payee": voucher.payee,
        "amount_str": str(voucher.amount),
        "description": voucher.description,
        "category": voucher.category.value,
        "approved_by": voucher.approved_by,
        "receipt_attached": voucher.receipt_attached,
        "status": voucher.status,
        "entered_by": voucher.entered_by,
        "created_at": voucher.created_at.isoformat(),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [r async for r in result]
    stored = _voucher_from_node(dict(records[0]["x"]))

    # Corresponding payment transaction (original behavior)
    transaction = PettyCashTransaction(
        id=str(uuid.uuid4()),
        fund_id=voucher.fund_id,
        transaction_type=TransactionType.PAYMENT,
        amount=voucher.amount,
        date=voucher.date,
        description=voucher.description,
        category=voucher.category,
        recipient_name=voucher.payee,
        reference_number=f"VOUCHER-{voucher.voucher_number}",
        voucher_number=voucher.voucher_number,
        entered_by=voucher.entered_by,
        created_at=_now(),
    )
    created_tx = await create_transaction(session, user_id, transaction, check_balance=False)
    return {"voucher": stored, "transaction_id": created_tx.id}


async def list_vouchers(
    session: AsyncSession,
    user_id: str,
    fund_id: Optional[str] = None,
    status: Optional[str] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
) -> List[PettyCashVoucher]:
    nodes = await _list_nodes(session, user_id, "PettyCashVoucher", "OWNS_VOUCHER")
    vouchers = [_voucher_from_node(n) for n in nodes]
    if fund_id:
        vouchers = [v for v in vouchers if v.fund_id == fund_id]
    if status:
        vouchers = [v for v in vouchers if v.status == status]
    if start_date:
        vouchers = [v for v in vouchers if v.date >= start_date]
    if end_date:
        vouchers = [v for v in vouchers if v.date <= end_date]
    vouchers.sort(key=lambda v: v.date, reverse=True)
    return vouchers


async def approve_voucher(session: AsyncSession, user_id: str, voucher_id: str, approved_by: str) -> PettyCashVoucher:
    node = await _get_node(session, user_id, "PettyCashVoucher", "OWNS_VOUCHER", voucher_id)
    if not node:
        raise NotFoundError("Voucher not found")
    set_lines = "x.approved_by = $approved_by,\n        x.status = $status"
    node = await _set_node_props(
        session,
        user_id,
        "PettyCashVoucher",
        "OWNS_VOUCHER",
        voucher_id,
        set_lines,
        {"approved_by": approved_by, "status": "approved"},
    )
    return _voucher_from_node(node)


# --- replenishments ---


async def create_replenishment(
    session: AsyncSession, user_id: str, repl: PettyCashReplenishment
) -> PettyCashReplenishment:
    repl.id = str(uuid.uuid4())
    repl.request_date = _now()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:PettyCashReplenishment {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        fund_id: $fund_id,
        amount_str: $amount_str,
        request_date: datetime($request_date),
        requested_by: $requested_by,
        approved_by: $approved_by,
        approved_date: datetime($approved_date),
        status: $status,
        transactions_included_json: $transactions_included_json,
        total_cash_disbursed_str: $total_cash_disbursed_str,
        bank_reference: $bank_reference,
        notes: $notes
    })
    CREATE (u)-[:OWNS_REPLENISHMENT]->(x)
    RETURN x
    """
    params = {
        "id": repl.id,
        "fund_id": repl.fund_id,
        "amount_str": str(repl.amount),
        "request_date": repl.request_date.isoformat(),
        "requested_by": repl.requested_by,
        "approved_by": repl.approved_by,
        "approved_date": repl.approved_date.isoformat() if repl.approved_date else None,
        "status": repl.status.value,
        "transactions_included_json": json.dumps(repl.transactions_included),
        "total_cash_disbursed_str": str(repl.total_cash_disbursed),
        "bank_reference": repl.bank_reference,
        "notes": repl.notes,
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [r async for r in result]
    stored = _repl_from_node(dict(records[0]["x"]))

    # Fund goes REPLENISHING (original behavior) when the fund is caller's
    fund_node = await _get_node(session, user_id, "PettyCashFund", "OWNS_FUND", repl.fund_id)
    if fund_node:
        set_lines = "x.status = $status,\n        x.updated_at = datetime($updated_at)"
        await _set_node_props(
            session,
            user_id,
            "PettyCashFund",
            "OWNS_FUND",
            repl.fund_id,
            set_lines,
            {"status": PettyCashStatus.REPLENISHING.value, "updated_at": _now().isoformat()},
        )
    return stored


async def list_replenishments(
    session: AsyncSession, user_id: str, fund_id: Optional[str] = None, status: Optional[ReimbursementStatus] = None
) -> List[PettyCashReplenishment]:
    nodes = await _list_nodes(session, user_id, "PettyCashReplenishment", "OWNS_REPLENISHMENT")
    repls = [_repl_from_node(n) for n in nodes]
    if fund_id:
        repls = [r for r in repls if r.fund_id == fund_id]
    if status is not None:
        repls = [r for r in repls if r.status == status]
    return repls


async def approve_replenishment(
    session: AsyncSession, user_id: str, replenishment_id: str, approved_by: str
) -> Dict[str, Any]:
    node = await _get_node(session, user_id, "PettyCashReplenishment", "OWNS_REPLENISHMENT", replenishment_id)
    if not node:
        raise NotFoundError("Replenishment not found")
    repl = _repl_from_node(node)
    set_lines = (
        "x.status = $status,\n        x.approved_by = $approved_by,\n        x.approved_date = datetime($approved_date)"
    )
    node = await _set_node_props(
        session,
        user_id,
        "PettyCashReplenishment",
        "OWNS_REPLENISHMENT",
        replenishment_id,
        set_lines,
        {
            "status": ReimbursementStatus.APPROVED.value,
            "approved_by": approved_by,
            "approved_date": _now().isoformat(),
        },
    )
    repl = _repl_from_node(node)

    # Replenishment transaction (original behavior)
    transaction = PettyCashTransaction(
        id=str(uuid.uuid4()),
        fund_id=repl.fund_id,
        transaction_type=TransactionType.REPLENISHMENT,
        amount=repl.amount,
        date=_now(),
        description=f"Replenishment #{repl.id}",
        category=PaymentCategory.MISCELLANEOUS,
        reference_number=f"REPL-{repl.id[:8]}",
        voucher_number="V-REPL",
        approved_by=approved_by,
        entered_by=repl.requested_by,
    )
    created_tx = await create_transaction(session, user_id, transaction)

    # Fund returns ACTIVE (original behavior)
    fund_node = await _get_node(session, user_id, "PettyCashFund", "OWNS_FUND", repl.fund_id)
    if fund_node:
        set_lines = "x.status = $status,\n        x.updated_at = datetime($updated_at)"
        await _set_node_props(
            session,
            user_id,
            "PettyCashFund",
            "OWNS_FUND",
            repl.fund_id,
            set_lines,
            {"status": PettyCashStatus.ACTIVE.value, "updated_at": _now().isoformat()},
        )
    return {"replenishment": repl, "transaction_id": created_tx.id}


# --- reports ---


async def fund_report(
    session: AsyncSession, user_id: str, fund_id: str, start_date: Optional[datetime], end_date: Optional[datetime]
) -> Dict[str, Any]:
    fund = await get_fund(session, user_id, fund_id)
    txs = [t for t in await list_transactions(session, user_id, fund_id=fund_id, limit=10**9)]
    if start_date:
        txs = [t for t in txs if t.date >= start_date]
    if end_date:
        txs = [t for t in txs if t.date <= end_date]

    by_category = {}
    for t in txs:
        cat = t.category.value
        if cat not in by_category:
            by_category[cat] = {"count": 0, "total": Decimal("0")}
        by_category[cat]["count"] += 1
        by_category[cat]["total"] += t.amount

    return {
        "fund": fund.model_dump(mode="json"),
        "period": {"start": start_date, "end": end_date},
        "total_transactions": len(txs),
        "by_category": by_category,
        "balance_info": await fund_balance(session, user_id, fund_id),
    }


async def cash_position(session: AsyncSession, user_id: str) -> Dict[str, Any]:
    funds_summary = []
    for fund in await list_funds(session, user_id):
        balance_info = await fund_balance(session, user_id, fund.id)
        current_balance = Decimal(balance_info["current_balance"])
        funds_summary.append(
            {
                "fund_id": fund.id,
                "fund_name": fund.fund_name,
                "location": fund.location,
                "custodian": fund.custodian_name,
                "current_balance": str(current_balance),
                "maximum_balance": str(fund.maximum_balance),
                "utilization_percentage": float(current_balance / fund.maximum_balance * 100),
                "status": fund.status.value,
            }
        )
    total_balance = sum(Decimal(f["current_balance"]) for f in funds_summary)
    return {
        "total_funds": len(funds_summary),
        "total_cash": str(total_balance),
        "funds": funds_summary,
    }


async def category_summary(
    session: AsyncSession, user_id: str, start_date: Optional[datetime], end_date: Optional[datetime]
) -> Dict[str, Any]:
    txs = [t for t in await list_transactions(session, user_id, limit=10**9)]
    if start_date:
        txs = [t for t in txs if t.date >= start_date]
    if end_date:
        txs = [t for t in txs if t.date <= end_date]

    category_summary = {}
    for t in txs:
        if t.transaction_type == TransactionType.PAYMENT:
            cat = t.category.value
            if cat not in category_summary:
                category_summary[cat] = {"count": 0, "total": Decimal("0")}
            category_summary[cat]["count"] += 1
            category_summary[cat]["total"] += t.amount

    return {
        "categories": category_summary,
        "total_transactions": sum(c["count"] for c in category_summary.values()),
        "total_amount": sum(c["total"] for c in category_summary.values()),
    }
