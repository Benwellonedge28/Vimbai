"""
Purchases Ledger Control Service CRUD Operations

Creditor transactions persist as :CreditorTransaction nodes, caller-owned
(X-User-Id) and Book-gated (X-Book-ID). Per-creditor running balances are
derived in Python from the caller's visible transaction set (identical to
the original sequential in-memory ledger), so no separate balance store is
needed. The label is distinct from the sales-ledger twin (:DebtorTransaction)
so the two services can never read each other's records even when sharing
a database.
"""

from typing import Dict, List, Optional

from neo4j import AsyncSession
from purchases_ledger_control_service.dependencies import book_id_var
from purchases_ledger_control_service.models import CreditorTransaction, TransactionType

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

# Sign conventions per the original sequential balance update.
_ADD_TYPES = {TransactionType.PURCHASE_INVOICE}
_SUB_TYPES = {TransactionType.DEBIT_NOTE, TransactionType.PAYMENT, TransactionType.REFUND}


async def _run(session: AsyncSession, query: str, params: Optional[Dict] = None, **kw):
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _as_dt(value):
    if value is None:
        return None
    if hasattr(value, "iso_format"):  # Temporal (real driver)
        iso = value.iso_format()
    elif hasattr(value, "isoformat"):
        iso = value.isoformat()
    else:
        iso = str(value)
    if iso is None or iso == "None":
        return None
    from datetime import datetime, timezone

    dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _txn_from_node(n: Dict) -> CreditorTransaction:
    return CreditorTransaction(
        id=n["id"],
        transaction_type=TransactionType(n["transaction_type"]),
        creditor_id=n["creditor_id"],
        creditor_name=n["creditor_name"],
        invoice_number=n.get("invoice_number"),
        date=_as_dt(n.get("date")),
        amount=float(n.get("amount", 0.0)),
        balance=float(n.get("balance", 0.0)),
        reference=n.get("reference"),
        journal_entry_id=n.get("journal_entry_id"),
        created_at=_as_dt(n.get("created_at")),
    )


async def list_transactions(session: AsyncSession, user_id: str) -> List[CreditorTransaction]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CREDITOR_TX]->(x:CreditorTransaction)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_txn_from_node(dict(r["x"])) async for r in result]


def derive_balances(transactions: List[CreditorTransaction]) -> Dict[str, float]:
    """Per-creditor running balance, replayed in creation order (original semantics)."""
    balances: Dict[str, float] = {}
    for t in transactions:
        if t.transaction_type in _ADD_TYPES:
            balances[t.creditor_id] = balances.get(t.creditor_id, 0.0) + t.amount
        elif t.transaction_type in _SUB_TYPES:
            balances[t.creditor_id] = balances.get(t.creditor_id, 0.0) - t.amount
    return balances


async def create_transaction(session: AsyncSession, user_id: str, txn: CreditorTransaction) -> CreditorTransaction:
    """Persist a creditor transaction. The stamped balance is set by the caller after deriving."""
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:CreditorTransaction {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        transaction_type: $transaction_type,
        creditor_id: $creditor_id,
        creditor_name: $creditor_name,
        invoice_number: $invoice_number,
        date: datetime($date),
        amount: toFloat($amount),
        balance: toFloat($balance),
        reference: $reference,
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_CREDITOR_TX]->(x)
    RETURN x
    """
    result = await _run(
        session,
        query,
        {
            "id": txn.id,
            "transaction_type": txn.transaction_type.value,
            "creditor_id": txn.creditor_id,
            "creditor_name": txn.creditor_name,
            "invoice_number": txn.invoice_number,
            "date": txn.date.isoformat(),
            "amount": txn.amount,
            "balance": txn.balance,
            "reference": txn.reference,
            "journal_entry_id": txn.journal_entry_id,
            "created_at": txn.created_at.isoformat(),
        },
        user_id=user_id,
    )
    records = [r async for r in result]
    return _txn_from_node(dict(records[0]["x"]))


async def update_journal_entry_id(
    session: AsyncSession, user_id: str, txn_id: str, journal_entry_id: Optional[str]
) -> None:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_CREDITOR_TX]->(x:CreditorTransaction)
    WHERE x.id = $txn_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.journal_entry_id = $journal_entry_id
    """
    await _run(session, query, {"journal_entry_id": journal_entry_id}, user_id=user_id, txn_id=txn_id)
