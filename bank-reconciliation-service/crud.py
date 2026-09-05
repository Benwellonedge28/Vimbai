"""
Bank Reconciliation Service CRUD Operations

Neo4j-backed persistence for:
- Bank statements (with statement lines)
- Cash book entries
- Bank reconciliations (with reconciliation items)

All records are stamped with book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)` so unscoped (personal)
requests see their own data and Book-scoped requests see only their Book.
Nested line/item collections are persisted as JSON properties.
"""

import json
import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from bank_reconciliation_service.dependencies import book_id_var
from bank_reconciliation_service.exceptions import ConflictError, NotFoundError, ValidationError
from bank_reconciliation_service.models import (
    BankReconciliation,
    BankStatement,
    BankStatementLine,
    CashBookEntry,
    MatchStatus,
    ReconciliationItem,
)
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"
NEO4J_URI = os.getenv("NEO4J_URI", "bolt://localhost:7687")


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


def _json_list(value) -> list:
    if not value:
        return []
    if isinstance(value, str):
        try:
            return json.loads(value)
        except (TypeError, ValueError):
            return []
    return list(value)


# ============================================================================
# Bank Statements
# ============================================================================


def _line_from_data(d: Dict[str, Any]) -> BankStatementLine:
    return BankStatementLine(**d)


def _statement_from_node(n: Dict[str, Any], user_id: str) -> BankStatement:
    return BankStatement(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        bank_account=n["bank_account"],
        statement_number=n["statement_number"],
        statement_start_date=_dt(n.get("statement_start_date")),
        statement_end_date=_dt(n.get("statement_end_date")),
        opening_balance=n.get("opening_balance", 0.0),
        closing_balance=n.get("closing_balance", 0.0),
        total_credits=n.get("total_credits", 0.0),
        total_debits=n.get("total_debits", 0.0),
        lines=[_line_from_data(d) for d in _json_list(n.get("lines"))],
        status=n.get("status", "imported"),
        created_at=_dt(n.get("created_at")),
    )


async def create_statement(session: AsyncSession, user_id: str, statement: BankStatement) -> BankStatement:
    statement_id = str(uuid.uuid4())
    created_at = _now()

    total_credits = sum(l.amount for l in statement.lines if not l.is_debit)
    total_debits = sum(l.amount for l in statement.lines if l.is_debit)

    lines_payload = [{**l.model_dump(mode="json"), "statement_id": statement_id} for l in statement.lines]

    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:BankStatement {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        bank_account: $bank_account,
        statement_number: $statement_number,
        statement_start_date: datetime($statement_start_date),
        statement_end_date: datetime($statement_end_date),
        opening_balance: toFloat($opening_balance),
        closing_balance: toFloat($closing_balance),
        total_credits: toFloat($total_credits),
        total_debits: toFloat($total_debits),
        lines: $lines,
        status: $status,
        created_at: datetime($created_at)
    }})
    CREATE (u)-[:OWNS_BANK_STATEMENT]->(x)
    RETURN x
    """
    params = {
        "id": statement_id,
        "user_id": user_id,
        "bank_account": statement.bank_account,
        "statement_number": statement.statement_number,
        "statement_start_date": statement.statement_start_date.isoformat(),
        "statement_end_date": statement.statement_end_date.isoformat(),
        "opening_balance": float(statement.opening_balance),
        "closing_balance": float(statement.closing_balance),
        "total_credits": float(total_credits),
        "total_debits": float(total_debits),
        "lines": json.dumps(lines_payload),
        "status": statement.status,
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    return _statement_from_node(record["x"], user_id)


async def get_statement(session: AsyncSession, user_id: str, statement_id: str) -> Optional[BankStatement]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_BANK_STATEMENT]->(x:BankStatement {{id: $id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, id=statement_id, user_id=user_id)
    record = await result.single()
    return _statement_from_node(record["x"], user_id) if record else None


async def list_statements(session: AsyncSession, user_id: str) -> List[BankStatement]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_BANK_STATEMENT]->(x:BankStatement)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_statement_from_node(r["x"], user_id) async for r in result]


async def save_statement_lines(session: AsyncSession, user_id: str, statement: BankStatement) -> None:
    """Persist mutated statement lines back to the node."""
    lines_payload = [l.model_dump(mode="json") for l in statement.lines]
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_BANK_STATEMENT]->(x:BankStatement {{id: $id}})
    {BOOK_FILTER}
    SET x.lines = $lines
    RETURN x
    """
    await _run(session, query, id=statement.id, user_id=user_id, lines=json.dumps(lines_payload))


# ============================================================================
# Cash Book Entries
# ============================================================================


def _cash_entry_from_node(n: Dict[str, Any], user_id: str) -> CashBookEntry:
    return CashBookEntry(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        date=_dt(n.get("date")),
        description=n["description"],
        reference=n["reference"],
        transaction_type=n.get("transaction_type", ""),
        amount=n.get("amount", 0.0),
        is_debit=bool(n.get("is_debit")),
        bank_reconciliation_id=n.get("bank_reconciliation_id"),
        matched=bool(n.get("matched", False)),
        matched_statement_id=n.get("matched_statement_id"),
    )


async def create_cash_book_entry(session: AsyncSession, user_id: str, entry: CashBookEntry) -> CashBookEntry:
    entry_id = str(uuid.uuid4())

    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:BankRecCashEntry {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        date: datetime($date),
        description: $description,
        reference: $reference,
        transaction_type: $transaction_type,
        amount: toFloat($amount),
        is_debit: $is_debit,
        bank_reconciliation_id: $bank_reconciliation_id,
        matched: $matched,
        matched_statement_id: $matched_statement_id,
        created_at: datetime($created_at)
    }})
    CREATE (u)-[:OWNS_BANKREC_CASH_ENTRY]->(x)
    RETURN x
    """
    params = {
        "id": entry_id,
        "user_id": user_id,
        "date": entry.date.isoformat(),
        "description": entry.description,
        "reference": entry.reference,
        "transaction_type": entry.transaction_type,
        "amount": float(entry.amount),
        "is_debit": entry.is_debit,
        "bank_reconciliation_id": None,
        "matched": False,
        "matched_statement_id": None,
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    return _cash_entry_from_node(record["x"], user_id)


async def list_cash_book_entries(session: AsyncSession, user_id: str) -> List[CashBookEntry]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_BANKREC_CASH_ENTRY]->(x:BankRecCashEntry)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_cash_entry_from_node(r["x"], user_id) async for r in result]


async def save_cash_book_entry(session: AsyncSession, user_id: str, entry: CashBookEntry) -> None:
    """Persist mutated match flags back to the node."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_BANKREC_CASH_ENTRY]->(x:BankRecCashEntry {{id: $id}})
    {BOOK_FILTER}
    SET x.matched = $matched,
        x.matched_statement_id = $matched_statement_id,
        x.bank_reconciliation_id = $bank_reconciliation_id
    RETURN x
    """
    await _run(
        session,
        query,
        id=entry.id,
        user_id=user_id,
        matched=entry.matched,
        matched_statement_id=entry.matched_statement_id,
        bank_reconciliation_id=entry.bank_reconciliation_id,
    )


# ============================================================================
# Bank Reconciliations
# ============================================================================


def _recon_from_node(n: Dict[str, Any], user_id: str) -> BankReconciliation:
    return BankReconciliation(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        bank_account=n["bank_account"],
        reconciliation_date=_dt(n.get("reconciliation_date")),
        statement_balance=n.get("statement_balance", 0.0),
        cash_book_balance=n.get("cash_book_balance", 0.0),
        difference=n.get("difference", 0.0),
        outstanding_deposits=n.get("outstanding_deposits", 0.0),
        outstanding_cheques=n.get("outstanding_cheques", 0.0),
        bank_errors=n.get("bank_errors", 0.0),
        cash_book_errors=n.get("cash_book_errors", 0.0),
        unpresented_cheques=n.get("unpresented_cheques", 0.0),
        uncredited_deposits=n.get("uncredited_deposits", 0.0),
        adjusted_statement_balance=n.get("adjusted_statement_balance", 0.0),
        adjusted_cash_book_balance=n.get("adjusted_cash_book_balance", 0.0),
        items=[ReconciliationItem(**d) for d in _json_list(n.get("items"))],
        status=n.get("status", "in_progress"),
        journal_entry_id=n.get("journal_entry_id"),
        created_at=_dt(n.get("created_at")),
        completed_at=_dt(n.get("completed_at")),
    )


async def create_reconciliation(
    session: AsyncSession,
    user_id: str,
    bank_account: str,
    reconciliation_date: datetime,
    statement_balance: float,
    cash_book_balance: float,
    outstanding_deposits: float = 0,
    outstanding_cheques: float = 0,
    bank_errors: float = 0,
    cash_book_errors: float = 0,
) -> BankReconciliation:
    reconciliation_id = str(uuid.uuid4())
    created_at = _now()

    adjusted_statement_balance = statement_balance - outstanding_cheques + outstanding_deposits - bank_errors
    adjusted_cash_book_balance = cash_book_balance - bank_errors + cash_book_errors
    difference = adjusted_statement_balance - adjusted_cash_book_balance

    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:BankReconciliation {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        bank_account: $bank_account,
        reconciliation_date: datetime($reconciliation_date),
        statement_balance: toFloat($statement_balance),
        cash_book_balance: toFloat($cash_book_balance),
        difference: toFloat($difference),
        outstanding_deposits: toFloat($outstanding_deposits),
        outstanding_cheques: toFloat($outstanding_cheques),
        bank_errors: toFloat($bank_errors),
        cash_book_errors: toFloat($cash_book_errors),
        unpresented_cheques: toFloat($unpresented_cheques),
        uncredited_deposits: toFloat($uncredited_deposits),
        adjusted_statement_balance: toFloat($adjusted_statement_balance),
        adjusted_cash_book_balance: toFloat($adjusted_cash_book_balance),
        items: $items,
        status: $status,
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at),
        completed_at: $completed_at
    }})
    CREATE (u)-[:OWNS_BANK_RECONCILIATION]->(x)
    RETURN x
    """
    params = {
        "id": reconciliation_id,
        "user_id": user_id,
        "bank_account": bank_account,
        "reconciliation_date": reconciliation_date.isoformat(),
        "statement_balance": float(statement_balance),
        "cash_book_balance": float(cash_book_balance),
        "difference": float(difference),
        "outstanding_deposits": float(outstanding_deposits),
        "outstanding_cheques": float(outstanding_cheques),
        "bank_errors": float(bank_errors),
        "cash_book_errors": float(cash_book_errors),
        "unpresented_cheques": 0.0,
        "uncredited_deposits": 0.0,
        "adjusted_statement_balance": float(adjusted_statement_balance),
        "adjusted_cash_book_balance": float(adjusted_cash_book_balance),
        "items": json.dumps([]),
        "status": "in_progress",
        "journal_entry_id": None,
        "created_at": created_at.isoformat(),
        "completed_at": None,
    }
    result = await _run(session, query, params)
    record = await result.single()
    return _recon_from_node(record["x"], user_id)


async def get_reconciliation(
    session: AsyncSession, user_id: str, reconciliation_id: str
) -> Optional[BankReconciliation]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_BANK_RECONCILIATION]->(x:BankReconciliation {{id: $id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, id=reconciliation_id, user_id=user_id)
    record = await result.single()
    return _recon_from_node(record["x"], user_id) if record else None


async def list_reconciliations(session: AsyncSession, user_id: str) -> List[BankReconciliation]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_BANK_RECONCILIATION]->(x:BankReconciliation)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_recon_from_node(r["x"], user_id) async for r in result]


async def save_reconciliation(session: AsyncSession, user_id: str, reconciliation: BankReconciliation) -> None:
    """Persist mutated items/status/journal info back to the node."""
    items_payload = [i.model_dump(mode="json") for i in reconciliation.items]
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_BANK_RECONCILIATION]->(x:BankReconciliation {{id: $id}})
    {BOOK_FILTER}
    SET x.items = $items,
        x.status = $status,
        x.journal_entry_id = $journal_entry_id,
        x.completed_at = datetime($completed_at)
    RETURN x
    """
    await _run(
        session,
        query,
        id=reconciliation.id,
        user_id=user_id,
        items=json.dumps(items_payload),
        status=reconciliation.status,
        journal_entry_id=reconciliation.journal_entry_id,
        completed_at=reconciliation.completed_at.isoformat() if reconciliation.completed_at else None,
    )
