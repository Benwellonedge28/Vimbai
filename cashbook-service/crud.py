"""
Cashbook Service CRUD Operations

Neo4j-backed persistence for:
- Bank / cash accounts
- Cash book entries (receipts & payments journals)
- Bank reconciliations
- Cash flow entries

All records are stamped with book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)` so unscoped (personal)
requests see their own data and Book-scoped requests see only their Book.
"""

import json
import os
import uuid
from datetime import datetime, timezone
from decimal import Decimal
from typing import Any, Dict, List, Optional

from cashbook_service.dependencies import book_id_var
from cashbook_service.exceptions import ConflictError, NotFoundError, ValidationError
from cashbook_service.models import (
    BankAccount,
    BankAccountType,
    BankReconciliation,
    CashBookEntry,
    CashBookType,
    CashFlowEntry,
    TransactionStatus,
)
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"
JSON_FIELDS_RECON = ("adjustments", "differences")


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _dt(value) -> datetime:
    if value is None:
        return None
    if isinstance(value, str):
        return datetime.fromisoformat(value)
    return datetime.fromisoformat(value.iso_format())


def _dec(value) -> Decimal:
    if value is None:
        return None
    return Decimal(str(value))


def _json(value) -> list:
    if value is None:
        return []
    if isinstance(value, str):
        try:
            return json.loads(value)
        except (TypeError, ValueError):
            return []
    return list(value)


# ============================================================================
# Bank Accounts
# ============================================================================


async def create_bank_account(session: AsyncSession, user_id: str, account: BankAccount) -> BankAccount:
    account_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    if await get_bank_account_by_code(session, user_id, account.account_code):
        raise ConflictError(detail=f"Account code {account.account_code} already exists", code="ACCOUNT_CODE_EXISTS")

    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:CashBankAccount {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        account_code: $account_code,
        account_name: $account_name,
        account_type: $account_type,
        bank_name: $bank_name,
        account_number: $account_number,
        currency: $currency,
        opening_balance: toFloat($opening_balance),
        current_balance: toFloat($current_balance),
        is_active: $is_active,
        reconciliation_enabled: $reconciliation_enabled,
        created_at: datetime($created_at)
    }})
    CREATE (u)-[:OWNS_CASH_ACCOUNT]->(x)
    RETURN x
    """
    params = {
        "id": account_id,
        "user_id": user_id,
        "account_code": account.account_code,
        "account_name": account.account_name,
        "account_type": account.account_type.value,
        "bank_name": account.bank_name,
        "account_number": account.account_number,
        "currency": account.currency,
        "opening_balance": float(account.opening_balance),
        "current_balance": float(account.opening_balance),
        "is_active": account.is_active,
        "reconciliation_enabled": account.reconciliation_enabled,
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    return _account_from_node(record["x"], user_id)


def _account_from_node(n: Dict[str, Any], user_id: str) -> BankAccount:
    return BankAccount(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        account_code=n["account_code"],
        account_name=n["account_name"],
        account_type=BankAccountType(n["account_type"]),
        bank_name=n.get("bank_name"),
        account_number=n.get("account_number"),
        currency=n.get("currency", "USD"),
        opening_balance=_dec(n.get("opening_balance")),
        current_balance=_dec(n.get("current_balance")),
        is_active=bool(n.get("is_active", True)),
        reconciliation_enabled=bool(n.get("reconciliation_enabled", True)),
        created_at=_dt(n.get("created_at")),
    )


async def get_bank_account(session: AsyncSession, user_id: str, account_id: str) -> Optional[BankAccount]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_ACCOUNT]->(x:CashBankAccount {{id: $id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, id=account_id, user_id=user_id)
    record = await result.single()
    return _account_from_node(record["x"], user_id) if record else None


async def get_bank_account_by_code(session: AsyncSession, user_id: str, account_code: str) -> Optional[BankAccount]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_ACCOUNT]->(x:CashBankAccount {{account_code: $account_code}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, account_code=account_code, user_id=user_id)
    record = await result.single()
    return _account_from_node(record["x"], user_id) if record else None


async def list_bank_accounts(session: AsyncSession, user_id: str) -> List[BankAccount]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_ACCOUNT]->(x:CashBankAccount)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_account_from_node(r["x"], user_id) async for r in result]


async def update_bank_account(
    session: AsyncSession, user_id: str, account_id: str, account: BankAccount
) -> Optional[BankAccount]:
    existing = await get_bank_account(session, user_id, account_id)
    if not existing:
        raise NotFoundError(detail="Account not found")

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_ACCOUNT]->(x:CashBankAccount {{id: $id}})
    {BOOK_FILTER}
    SET x.account_name = $account_name,
        x.account_type = $account_type,
        x.bank_name = $bank_name,
        x.account_number = $account_number,
        x.currency = $currency,
        x.opening_balance = toFloat($opening_balance),
        x.current_balance = toFloat($current_balance),
        x.is_active = $is_active,
        x.reconciliation_enabled = $reconciliation_enabled
    RETURN x
    """
    params = {
        "id": account_id,
        "user_id": user_id,
        "account_name": account.account_name,
        "account_type": account.account_type.value,
        "bank_name": account.bank_name,
        "account_number": account.account_number,
        "currency": account.currency,
        "opening_balance": float(account.opening_balance),
        "current_balance": float(account.current_balance),
        "is_active": account.is_active,
        "reconciliation_enabled": account.reconciliation_enabled,
    }
    await _run(session, query, params)
    return await get_bank_account(session, user_id, account_id)


async def adjust_account_balance(session: AsyncSession, user_id: str, account_id: str, new_balance: Decimal) -> None:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_ACCOUNT]->(x:CashBankAccount {{id: $id}})
    {BOOK_FILTER}
    SET x.current_balance = toFloat($current_balance)
    RETURN x
    """
    await _run(session, query, id=account_id, user_id=user_id, current_balance=float(new_balance))


# ============================================================================
# Cash Book Entries
# ============================================================================


def _entry_from_node(n: Dict[str, Any], user_id: str) -> CashBookEntry:
    return CashBookEntry(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        book_type=CashBookType(n["book_type"]),
        entry_date=_dt(n.get("entry_date")),
        voucher_number=n["voucher_number"],
        description=n["description"],
        account_code=n["account_code"],
        amount=_dec(n.get("amount")),
        is_debit=bool(n.get("is_debit")),
        reference=n.get("reference"),
        cheque_number=n.get("cheque_number"),
        bank_account=n.get("bank_account"),
        currency=n.get("currency", "USD"),
        exchange_rate=_dec(n.get("exchange_rate")) or Decimal("1.0"),
        base_amount=_dec(n.get("base_amount")),
        narration=n.get("narration"),
        posted_by=n["posted_by"],
        status=TransactionStatus(n.get("status", "pending")),
        reconciled=bool(n.get("reconciled", False)),
        reconciliation_id=n.get("reconciliation_id"),
        created_at=_dt(n.get("created_at")),
    )


async def create_cash_book_entry(session: AsyncSession, user_id: str, entry: CashBookEntry) -> CashBookEntry:
    entry_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    # Calculate base amount if multi-currency
    if entry.currency != "USD":
        base_amount = entry.amount * entry.exchange_rate
    else:
        base_amount = entry.amount

    # Update bank account balance if linked
    if entry.bank_account:
        account = await get_bank_account_by_code(session, user_id, entry.bank_account)
        if account:
            if entry.is_debit:
                account.current_balance += base_amount
            else:
                account.current_balance -= base_amount
            await adjust_account_balance(session, user_id, account.id, account.current_balance)

    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:CashBookEntry {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        book_type: $book_type,
        entry_date: datetime($entry_date),
        voucher_number: $voucher_number,
        description: $description,
        account_code: $account_code,
        amount: toFloat($amount),
        is_debit: $is_debit,
        reference: $reference,
        cheque_number: $cheque_number,
        bank_account: $bank_account,
        currency: $currency,
        exchange_rate: toFloat($exchange_rate),
        base_amount: toFloat($base_amount),
        narration: $narration,
        posted_by: $posted_by,
        status: $status,
        reconciled: $reconciled,
        reconciliation_id: $reconciliation_id,
        created_at: datetime($created_at)
    }})
    CREATE (u)-[:OWNS_CASH_ENTRY]->(x)
    RETURN x
    """
    params = {
        "id": entry_id,
        "user_id": user_id,
        "book_type": entry.book_type.value,
        "entry_date": entry.entry_date.isoformat(),
        "voucher_number": entry.voucher_number,
        "description": entry.description,
        "account_code": entry.account_code,
        "amount": float(entry.amount),
        "is_debit": entry.is_debit,
        "reference": entry.reference,
        "cheque_number": entry.cheque_number,
        "bank_account": entry.bank_account,
        "currency": entry.currency,
        "exchange_rate": float(entry.exchange_rate),
        "base_amount": float(base_amount),
        "narration": entry.narration,
        "posted_by": entry.posted_by,
        "status": entry.status.value,
        "reconciled": False,
        "reconciliation_id": None,
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    out = _entry_from_node(record["x"], user_id)
    return out


async def get_cash_book_entry(session: AsyncSession, user_id: str, entry_id: str) -> Optional[CashBookEntry]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_ENTRY]->(x:CashBookEntry {{id: $id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, id=entry_id, user_id=user_id)
    record = await result.single()
    return _entry_from_node(record["x"], user_id) if record else None


async def list_cash_book_entries(session: AsyncSession, user_id: str, limit: int = 100) -> List[CashBookEntry]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_ENTRY]->(x:CashBookEntry)
    {BOOK_FILTER}
    RETURN x
    LIMIT $limit
    """
    result = await _run(session, query, user_id=user_id, limit=limit)
    return [_entry_from_node(r["x"], user_id) async for r in result]


async def set_entry_status(
    session: AsyncSession, user_id: str, entry_id: str, status: TransactionStatus
) -> Optional[CashBookEntry]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_ENTRY]->(x:CashBookEntry {{id: $id}})
    {BOOK_FILTER}
    SET x.status = $status
    RETURN x
    """
    await _run(session, query, id=entry_id, user_id=user_id, status=status.value)
    return await get_cash_book_entry(session, user_id, entry_id)


async def mark_entry_reconciled(session: AsyncSession, user_id: str, entry_id: str, reconciliation_id: str) -> None:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_ENTRY]->(x:CashBookEntry {{id: $id}})
    {BOOK_FILTER}
    SET x.reconciled = $reconciled,
        x.reconciliation_id = $reconciliation_id
    RETURN x
    """
    await _run(session, query, id=entry_id, user_id=user_id, reconciled=True, reconciliation_id=reconciliation_id)


# ============================================================================
# Bank Reconciliations
# ============================================================================


def _recon_from_node(n: Dict[str, Any], user_id: str) -> BankReconciliation:
    return BankReconciliation(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        bank_account=n["bank_account"],
        statement_date=_dt(n.get("statement_date")),
        statement_balance=_dec(n.get("statement_balance")),
        book_balance=_dec(n.get("book_balance")),
        adjustments=_json(n.get("adjustments")),
        adjusted_balance=_dec(n.get("adjusted_balance")),
        differences=_json(n.get("differences")),
        status=n.get("status", "in_progress"),
        prepared_by=n["prepared_by"],
        reviewed_by=n.get("reviewed_by"),
        completed_at=_dt(n.get("completed_at")),
        created_at=_dt(n.get("created_at")),
    )


async def create_reconciliation(
    session: AsyncSession, user_id: str, reconciliation: BankReconciliation
) -> BankReconciliation:
    reconciliation_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    # Calculate book balance from posted entries
    book_balance = Decimal("0")
    for e in await _iter_entries_for_account(session, user_id, reconciliation.bank_account):
        if e.status == TransactionStatus.POSTED:
            book_balance += e.base_amount if e.is_debit else -e.base_amount

    account = await get_bank_account_by_code(session, user_id, reconciliation.bank_account)
    if account:
        book_balance = account.opening_balance + book_balance

    total_adjustments = sum(Decimal(str(a.get("amount", 0))) for a in reconciliation.adjustments)
    adjusted_balance = reconciliation.statement_balance + total_adjustments

    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:CashBookReconciliation {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        bank_account: $bank_account,
        statement_date: datetime($statement_date),
        statement_balance: toFloat($statement_balance),
        book_balance: toFloat($book_balance),
        adjustments: $adjustments,
        adjusted_balance: toFloat($adjusted_balance),
        differences: $differences,
        status: $status,
        prepared_by: $prepared_by,
        reviewed_by: $reviewed_by,
        completed_at: $completed_at,
        created_at: datetime($created_at)
    }})
    CREATE (u)-[:OWNS_CASH_RECON]->(x)
    RETURN x
    """
    params = {
        "id": reconciliation_id,
        "user_id": user_id,
        "bank_account": reconciliation.bank_account,
        "statement_date": reconciliation.statement_date.isoformat(),
        "statement_balance": float(reconciliation.statement_balance),
        "book_balance": float(book_balance),
        "adjustments": json.dumps(reconciliation.adjustments),
        "adjusted_balance": float(adjusted_balance),
        "differences": json.dumps(reconciliation.differences),
        "status": reconciliation.status,
        "prepared_by": reconciliation.prepared_by,
        "reviewed_by": None,
        "completed_at": None,
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    return _recon_from_node(record["x"], user_id)


async def _iter_entries_for_account(session: AsyncSession, user_id: str, bank_account: str):
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_ENTRY]->(x:CashBookEntry {{bank_account: $bank_account}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, bank_account=bank_account)
    return [_entry_from_node(r["x"], user_id) async for r in result]


async def get_reconciliation(
    session: AsyncSession, user_id: str, reconciliation_id: str
) -> Optional[BankReconciliation]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_RECON]->(x:CashBookReconciliation {{id: $id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, id=reconciliation_id, user_id=user_id)
    record = await result.single()
    return _recon_from_node(record["x"], user_id) if record else None


async def list_reconciliations(session: AsyncSession, user_id: str) -> List[BankReconciliation]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_RECON]->(x:CashBookReconciliation)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_recon_from_node(r["x"], user_id) async for r in result]


async def complete_reconciliation(
    session: AsyncSession, user_id: str, reconciliation_id: str, reviewed_by: str
) -> Optional[BankReconciliation]:
    existing = await get_reconciliation(session, user_id, reconciliation_id)
    if not existing:
        raise NotFoundError(detail="Reconciliation not found")

    completed_at = datetime.now(timezone.utc)
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_RECON]->(x:CashBookReconciliation {{id: $id}})
    {BOOK_FILTER}
    SET x.status = $status,
        x.reviewed_by = $reviewed_by,
        x.completed_at = datetime($completed_at)
    RETURN x
    """
    await _run(
        session,
        query,
        id=reconciliation_id,
        user_id=user_id,
        status="completed",
        reviewed_by=reviewed_by,
        completed_at=completed_at.isoformat(),
    )

    # Mark referenced entries as reconciled
    for adjustment in existing.adjustments:
        if "entry_id" in adjustment:
            entry = await get_cash_book_entry(session, user_id, adjustment["entry_id"])
            if entry:
                await mark_entry_reconciled(session, user_id, entry.id, reconciliation_id)

    return await get_reconciliation(session, user_id, reconciliation_id)


# ============================================================================
# Cash Flow Entries
# ============================================================================


def _flow_from_node(n: Dict[str, Any], user_id: str) -> CashFlowEntry:
    return CashFlowEntry(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        entry_date=_dt(n.get("entry_date")),
        category=n["category"],
        subcategory=n.get("subcategory"),
        description=n["description"],
        expected_amount=_dec(n.get("expected_amount")),
        actual_amount=_dec(n.get("actual_amount")),
        variance=_dec(n.get("variance")),
        cash_flow_type=n["cash_flow_type"],
        source=n["source"],
        reference_id=n.get("reference_id"),
        created_at=_dt(n.get("created_at")),
    )


async def create_cash_flow_entry(session: AsyncSession, user_id: str, entry: CashFlowEntry) -> CashFlowEntry:
    entry_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    variance = None
    if entry.expected_amount is not None and entry.actual_amount is not None:
        variance = entry.actual_amount - entry.expected_amount

    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:CashFlowEntry {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        entry_date: datetime($entry_date),
        category: $category,
        subcategory: $subcategory,
        description: $description,
        expected_amount: toFloat($expected_amount),
        actual_amount: toFloat($actual_amount),
        variance: toFloat($variance),
        cash_flow_type: $cash_flow_type,
        source: $source,
        reference_id: $reference_id,
        created_at: datetime($created_at)
    }})
    CREATE (u)-[:OWNS_CASH_FLOW]->(x)
    RETURN x
    """
    params = {
        "id": entry_id,
        "user_id": user_id,
        "entry_date": entry.entry_date.isoformat(),
        "category": entry.category,
        "subcategory": entry.subcategory,
        "description": entry.description,
        "expected_amount": float(entry.expected_amount) if entry.expected_amount is not None else 0.0,
        "actual_amount": float(entry.actual_amount) if entry.actual_amount is not None else 0.0,
        "variance": float(variance) if variance is not None else 0.0,
        "cash_flow_type": entry.cash_flow_type,
        "source": entry.source,
        "reference_id": entry.reference_id,
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    return _flow_from_node(record["x"], user_id)


async def list_cash_flow_entries(session: AsyncSession, user_id: str, limit: int = 100) -> List[CashFlowEntry]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_FLOW]->(x:CashFlowEntry)
    {BOOK_FILTER}
    RETURN x
    LIMIT $limit
    """
    result = await _run(session, query, user_id=user_id, limit=limit)
    return [_flow_from_node(r["x"], user_id) async for r in result]
