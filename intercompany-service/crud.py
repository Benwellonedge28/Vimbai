"""
Intercompany Service CRUD Operations

Entities, transactions and elimination entries persist in Neo4j,
caller-owned (X-User-Id) and Book-gated (X-Book-ID, verified upstream by
the API gateway). Previously the three shared module-level lists let any
caller register entities, create transactions and match/eliminate anyone
else's intercompany pairs.
"""

from typing import Dict, List, Optional

from intercompany_service.dependencies import book_id_var
from intercompany_service.models import EliminationEntry, IntercompanyEntity, IntercompanyTransaction
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session: AsyncSession, query: str, params: Optional[Dict] = None, **kw):
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _as_dt(value) -> Optional[object]:
    if value is None:
        return None
    if hasattr(value, "iso_format"):  # Temporal
        iso = value.iso_format()
        if iso is None or iso == "None":
            return None
        from datetime import datetime, timezone

        dt = datetime.fromisoformat(iso)
        return dt if dt.tzinfo else dt.replace(tzinfo=timezone.utc)
    return value


def _iso(dt) -> Optional[str]:
    if dt is None:
        return None
    if hasattr(dt, "iso_format"):  # Temporal
        return dt.iso_format()
    if getattr(dt, "tzinfo", None) is None:
        from datetime import timezone

        dt = dt.replace(tzinfo=timezone.utc)
    return dt.isoformat()


# --- entities ---


def _entity_from_node(n: Dict) -> IntercompanyEntity:
    return IntercompanyEntity(
        id=n["id"],
        name=n.get("name", ""),
        legal_entity_code=n.get("legal_entity_code", ""),
        tax_jurisdiction=n.get("tax_jurisdiction", ""),
        currency=n.get("currency", "USD"),
        status=n.get("status", "active"),
        created_at=_as_dt(n.get("created_at")),
    )


async def create_entity(session: AsyncSession, user_id: str, e: IntercompanyEntity) -> IntercompanyEntity:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:IntercompanyEntity {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        legal_entity_code: $legal_entity_code,
        tax_jurisdiction: $tax_jurisdiction,
        currency: $currency,
        status: $status,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_ENTITY]->(x)
    RETURN x
    """
    params = {
        "id": e.id,
        "name": e.name,
        "legal_entity_code": e.legal_entity_code,
        "tax_jurisdiction": e.tax_jurisdiction,
        "currency": e.currency,
        "status": e.status,
        "created_at": _iso(e.created_at),
    }
    result = await _run(session, query, params, user_id=user_id)
    rec = await result.single()
    return _entity_from_node(dict(rec["x"]))


async def list_entities(session: AsyncSession, user_id: str) -> List[IntercompanyEntity]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ENTITY]->(x:IntercompanyEntity)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_entity_from_node(dict(rec["x"])) async for rec in result]


# --- transactions ---


def _transaction_from_node(n: Dict) -> IntercompanyTransaction:
    return IntercompanyTransaction(
        id=n["id"],
        from_entity_id=n.get("from_entity_id", ""),
        to_entity_id=n.get("to_entity_id", ""),
        transaction_type=n.get("transaction_type", ""),
        amount=float(n.get("amount", 0.0)),
        currency=n.get("currency", "USD"),
        description=n.get("description", ""),
        transfer_price_basis=n.get("transfer_price_basis", "cost_plus"),
        transaction_date=_as_dt(n.get("transaction_date")),
        status=n.get("status", "pending"),
        matched_transaction_id=n.get("matched_transaction_id"),
    )


async def create_transaction(
    session: AsyncSession, user_id: str, t: IntercompanyTransaction
) -> IntercompanyTransaction:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:IntercompanyTransaction {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        from_entity_id: $from_entity_id,
        to_entity_id: $to_entity_id,
        transaction_type: $transaction_type,
        amount: toFloat($amount),
        currency: $currency,
        description: $description,
        transfer_price_basis: $transfer_price_basis,
        transaction_date: datetime($transaction_date),
        status: $status,
        matched_transaction_id: $matched_transaction_id
    })
    CREATE (u)-[:OWNS_TRANSACTION]->(x)
    RETURN x
    """
    params = {
        "id": t.id,
        "from_entity_id": t.from_entity_id,
        "to_entity_id": t.to_entity_id,
        "transaction_type": t.transaction_type,
        "amount": float(t.amount),
        "currency": t.currency,
        "description": t.description,
        "transfer_price_basis": t.transfer_price_basis,
        "transaction_date": _iso(t.transaction_date),
        "status": t.status,
        "matched_transaction_id": t.matched_transaction_id,
    }
    result = await _run(session, query, params, user_id=user_id)
    rec = await result.single()
    return _transaction_from_node(dict(rec["x"]))


async def get_transaction(session: AsyncSession, user_id: str, txn_id: str) -> Optional[IntercompanyTransaction]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_TRANSACTION]->(x:IntercompanyTransaction)
    WHERE x.id = $txn_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, txn_id=txn_id, user_id=user_id)
    rec = await result.single()
    return _transaction_from_node(dict(rec["x"])) if rec else None


async def list_transactions(session: AsyncSession, user_id: str) -> List[IntercompanyTransaction]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_TRANSACTION]->(x:IntercompanyTransaction)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_transaction_from_node(dict(rec["x"])) async for rec in result]


async def mark_matched(session: AsyncSession, user_id: str, txn_id: str, other_txn_id: str) -> None:
    """Persist matched status + back-reference on a transaction."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_TRANSACTION]->(x:IntercompanyTransaction)
    WHERE x.id = $txn_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.status = $status,
        x.matched_transaction_id = $matched_transaction_id
    """
    await _run(
        session,
        query,
        txn_id=txn_id,
        status="matched",
        matched_transaction_id=other_txn_id,
        user_id=user_id,
    )


# --- eliminations ---


def _elimination_from_node(n: Dict) -> EliminationEntry:
    return EliminationEntry(
        id=n["id"],
        pair_id=n.get("pair_id", ""),
        debit_entity_id=n.get("debit_entity_id", ""),
        credit_entity_id=n.get("credit_entity_id", ""),
        amount=float(n.get("amount", 0.0)),
        description=n.get("description", ""),
        elimination_date=_as_dt(n.get("elimination_date")),
    )


async def create_elimination(session: AsyncSession, user_id: str, e: EliminationEntry) -> EliminationEntry:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:EliminationEntry {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        pair_id: $pair_id,
        debit_entity_id: $debit_entity_id,
        credit_entity_id: $credit_entity_id,
        amount: toFloat($amount),
        description: $description,
        elimination_date: datetime($elimination_date)
    })
    CREATE (u)-[:OWNS_ELIMINATION]->(x)
    RETURN x
    """
    params = {
        "id": e.id,
        "pair_id": e.pair_id,
        "debit_entity_id": e.debit_entity_id,
        "credit_entity_id": e.credit_entity_id,
        "amount": float(e.amount),
        "description": e.description,
        "elimination_date": _iso(e.elimination_date),
    }
    result = await _run(session, query, params, user_id=user_id)
    rec = await result.single()
    return _elimination_from_node(dict(rec["x"]))


async def list_eliminations(session: AsyncSession, user_id: str) -> List[EliminationEntry]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ELIMINATION]->(x:EliminationEntry)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_elimination_from_node(dict(rec["x"])) async for rec in result]
