"""
Bank Feed Integration Service CRUD Operations

Neo4j-backed persistence for bank connections, imported transactions,
reconciliation rules, and sync history. Every record is stamped with
user_id + book_id; every read applies caller ownership plus the Book filter.
Cross-scope access is 404 (no existence leak). Bank-side helpers (MT940
parsing, webhook signatures, provider fetch stubs) stay pure.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from bank_feed_service.dependencies import book_id_var
from bank_feed_service.exceptions import ConflictError, NotFoundError
from bank_feed_service.models import (
    BankConnection,
    BankConnectionCreate,
    BankProvider,
    ReconciliationRule,
    SyncResult,
    SyncStatus,
    TransactionImport,
    TransactionInDB,
    TransactionStatus,
)
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
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


# --- hydration ---


def _conn_from_node(n: Dict[str, Any]) -> BankConnection:
    return BankConnection(
        id=n["id"],
        book_id=n.get("book_id"),
        organization_id=n["organization_id"],
        provider=BankProvider(n["provider"]),
        account_name=n["account_name"],
        account_type=n["account_type"],
        account_number_last4=n["account_number_last4"],
        routing_number=n.get("routing_number"),
        access_token_encrypted=n.get("access_token_encrypted"),
        webhook_url=n.get("webhook_url"),
        auto_sync_enabled=bool(n.get("auto_sync_enabled", True)),
        sync_interval_minutes=int(n.get("sync_interval_minutes", 60)),
        status=n.get("status", "active"),
        last_sync_at=_as_dt(n.get("last_sync_at")),
        last_sync_status=SyncStatus(n["last_sync_status"]) if n.get("last_sync_status") else None,
        error_message=n.get("error_message"),
        created_at=_as_dt(n.get("created_at")) or _now(),
        updated_at=_as_dt(n.get("updated_at")) or _now(),
    )


def _tx_from_node(n: Dict[str, Any]) -> TransactionInDB:
    metadata = json.loads(n.get("metadata_json") or "null")
    return TransactionInDB(
        id=n["id"],
        book_id=n.get("book_id"),
        bank_connection_id=n["bank_connection_id"],
        external_id=n["external_id"],
        date=_as_dt(n.get("date")) or _now(),
        amount=float(n.get("amount", 0)),
        currency=n.get("currency", "USD"),
        description=n.get("description", ""),
        category=n.get("category"),
        merchant_name=n.get("merchant_name"),
        merchant_id=n.get("merchant_id"),
        transaction_type=n.get("transaction_type", "debit"),
        pending=bool(n.get("pending", False)),
        metadata=metadata,
        linked_journal_entry_id=n.get("linked_journal_entry_id"),
        linked_invoice_id=n.get("linked_invoice_id"),
        matched_rule_id=n.get("matched_rule_id"),
        status=TransactionStatus(n.get("status", "pending")),
        confidence_score=float(n.get("confidence_score", 0.0)),
        imported_at=_as_dt(n.get("imported_at")) or _now(),
        created_at=_as_dt(n.get("created_at")) or _now(),
        updated_at=_as_dt(n.get("updated_at")) or _now(),
    )


def _rule_from_node(n: Dict[str, Any]) -> ReconciliationRule:
    return ReconciliationRule(
        id=n["id"],
        book_id=n.get("book_id"),
        name=n["name"],
        description=n.get("description"),
        match_conditions=json.loads(n.get("match_conditions_json") or "{}"),
        priority=int(n.get("priority", 0)),
        auto_match_enabled=bool(n.get("auto_match_enabled", True)),
        create_journal_entry=bool(n.get("create_journal_entry", False)),
        journal_entry_template=json.loads(n.get("journal_entry_template_json") or "null"),
        active=bool(n.get("active", True)),
    )


def _sync_from_node(n: Dict[str, Any]) -> SyncResult:
    return SyncResult(
        sync_id=n["id"],
        book_id=n.get("book_id"),
        bank_connection_id=n["bank_connection_id"],
        status=SyncStatus(n.get("status", "pending")),
        transactions_imported=int(n.get("transactions_imported", 0)),
        transactions_updated=int(n.get("transactions_updated", 0)),
        transactions_matched=int(n.get("transactions_matched", 0)),
        errors=json.loads(n.get("errors_json") or "[]"),
        started_at=_as_dt(n.get("started_at")) or _now(),
        completed_at=_as_dt(n.get("completed_at")),
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


async def _get_node(session: AsyncSession, user_id: str, label: str, edge: str, node_id: str) -> Optional[Dict]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    WHERE x.id = $node_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, node_id=node_id)
    record = await result.single()
    return dict(record["x"]) if record else None


async def _set_node_props(
    session: AsyncSession, user_id: str, label: str, edge: str, node_id: str, set_lines: str, params: Dict[str, Any]
) -> Dict[str, Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    WHERE x.id = $node_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET {set_lines}
    RETURN x
    """
    merged = dict(params)
    merged.update({"user_id": user_id, "node_id": node_id})
    result = await _run(session, query, merged)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Record not found")
    return dict(records[0]["x"])


async def _delete_node(session: AsyncSession, user_id: str, label: str, edge: str, node_id: str) -> None:
    """Caller-owned, Book-gated delete: 404 first if invisible, then DETACH DELETE."""
    node = await _get_node(session, user_id, label, edge, node_id)  # 404 if not caller's
    if not node:
        raise NotFoundError("Record not found")
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    WHERE x.id = $node_id AND ($book_id IS NULL OR x.book_id = $book_id)
    DETACH DELETE x
    """
    await _run(session, query, user_id=user_id, node_id=node_id)


# --- connections ---


async def create_connection(
    session: AsyncSession, user_id: str, payload: BankConnectionCreate, organization_id: str
) -> BankConnection:
    conn_id = str(uuid.uuid4())
    now = _now()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:BankConnection {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        organization_id: $organization_id,
        provider: $provider,
        account_name: $account_name,
        account_type: $account_type,
        account_number_last4: $account_number_last4,
        routing_number: $routing_number,
        access_token_encrypted: $access_token_encrypted,
        webhook_url: $webhook_url,
        auto_sync_enabled: $auto_sync_enabled,
        sync_interval_minutes: toInteger($sync_interval_minutes),
        status: $status,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_CONNECTION]->(x)
    RETURN x
    """
    params = {
        "id": conn_id,
        "organization_id": organization_id,
        "provider": payload.provider.value,
        "account_name": payload.account_name,
        "account_type": payload.account_type.value,
        "account_number_last4": payload.account_number_last4,
        "routing_number": payload.routing_number,
        "access_token_encrypted": payload.access_token_encrypted,
        "webhook_url": payload.webhook_url,
        "auto_sync_enabled": payload.auto_sync_enabled,
        "sync_interval_minutes": payload.sync_interval_minutes,
        "status": "active",
        "created_at": now.isoformat(),
        "updated_at": now.isoformat(),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [r async for r in result]
    return _conn_from_node(dict(records[0]["x"]))


async def list_connections(session: AsyncSession, user_id: str, organization_id: str) -> List[BankConnection]:
    nodes = await _list_nodes(session, user_id, "BankConnection", "OWNS_CONNECTION")
    conns = [_conn_from_node(n) for n in nodes if n.get("organization_id") == organization_id]
    return conns


async def get_connection(session: AsyncSession, user_id: str, connection_id: str) -> BankConnection:
    node = await _get_node(session, user_id, "BankConnection", "OWNS_CONNECTION", connection_id)
    if not node:
        raise NotFoundError("Connection not found")
    return _conn_from_node(node)


async def delete_connection(session: AsyncSession, user_id: str, connection_id: str) -> None:
    await _delete_node(session, user_id, "BankConnection", "OWNS_CONNECTION", connection_id)


# --- transactions ---


async def import_transaction(session: AsyncSession, user_id: str, payload: TransactionImport) -> TransactionInDB:
    await get_connection(session, user_id, payload.bank_connection_id)  # 404 if not caller's

    # Duplicate check scoped to caller + connection (original semantics)
    txs = [
        t
        for t in await _list_txs(session, user_id)
        if t.external_id == payload.external_id and t.bank_connection_id == payload.bank_connection_id
    ]
    if txs:
        raise ConflictError("Transaction already imported", existing_id=txs[0].id)

    tx_id = str(uuid.uuid4())
    now = _now()
    status_value = TransactionStatus.PENDING if payload.pending else TransactionStatus.CLEARED
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:BankFeedTransaction {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        bank_connection_id: $bank_connection_id,
        external_id: $external_id,
        date: datetime($date),
        amount: toFloat($amount),
        currency: $currency,
        description: $description,
        category: $category,
        merchant_name: $merchant_name,
        merchant_id: $merchant_id,
        transaction_type: $transaction_type,
        pending: $pending,
        metadata_json: $metadata_json,
        status: $status,
        confidence_score: toFloat($confidence_score),
        imported_at: datetime($imported_at),
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_BANK_TX]->(x)
    RETURN x
    """
    params = {
        "id": tx_id,
        "bank_connection_id": payload.bank_connection_id,
        "external_id": payload.external_id,
        "date": payload.date.isoformat(),
        "amount": payload.amount,
        "currency": payload.currency,
        "description": payload.description,
        "category": payload.category,
        "merchant_name": payload.merchant_name,
        "merchant_id": payload.merchant_id,
        "transaction_type": payload.transaction_type,
        "pending": payload.pending,
        "metadata_json": json.dumps(payload.metadata),
        "status": status_value.value,
        "confidence_score": 0.0,
        "imported_at": now.isoformat(),
        "created_at": now.isoformat(),
        "updated_at": now.isoformat(),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [r async for r in result]
    tx = _tx_from_node(dict(records[0]["x"]))

    # Auto-match against the caller's own rules (original behavior, now scoped)
    await apply_rules_to_tx(session, user_id, tx)
    refreshed = await _get_node(session, user_id, "BankFeedTransaction", "OWNS_BANK_TX", tx.id)
    return _tx_from_node(refreshed) if refreshed else tx


async def _list_txs(session: AsyncSession, user_id: str) -> List[TransactionInDB]:
    nodes = await _list_nodes(session, user_id, "BankFeedTransaction", "OWNS_BANK_TX")
    return [_tx_from_node(n) for n in nodes]


async def list_transactions(
    session: AsyncSession,
    user_id: str,
    connection_id: Optional[str] = None,
    status: Optional[TransactionStatus] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100,
    offset: int = 0,
) -> Dict[str, Any]:
    results = await _list_txs(session, user_id)
    if connection_id:
        results = [t for t in results if t.bank_connection_id == connection_id]
    if status:
        results = [t for t in results if t.status == status]
    if start_date:
        results = [t for t in results if t.date >= start_date]
    if end_date:
        results = [t for t in results if t.date <= end_date]
    results.sort(key=lambda x: x.date, reverse=True)
    total = len(results)
    return {"total": total, "transactions": results[offset : offset + limit]}


async def get_transaction(session: AsyncSession, user_id: str, transaction_id: str) -> TransactionInDB:
    node = await _get_node(session, user_id, "BankFeedTransaction", "OWNS_BANK_TX", transaction_id)
    if not node:
        raise NotFoundError("Transaction not found")
    return _tx_from_node(node)


async def update_transaction_status(
    session: AsyncSession, user_id: str, transaction_id: str, status: TransactionStatus, notes: Optional[str]
) -> TransactionInDB:
    node = await _get_node(session, user_id, "BankFeedTransaction", "OWNS_BANK_TX", transaction_id)
    if not node:
        raise NotFoundError("Transaction not found")
    tx = _tx_from_node(node)
    metadata = dict(tx.metadata or {})
    if notes:
        metadata["status_notes"] = notes
    set_lines = (
        "x.status = $status,\n        x.metadata_json = $metadata_json,\n        "
        "x.updated_at = datetime($updated_at)"
    )
    node = await _set_node_props(
        session,
        user_id,
        "BankFeedTransaction",
        "OWNS_BANK_TX",
        transaction_id,
        set_lines,
        {
            "status": status.value,
            "metadata_json": json.dumps(metadata),
            "updated_at": _now().isoformat(),
        },
    )
    return _tx_from_node(node)


async def link_transaction(
    session: AsyncSession,
    user_id: str,
    transaction_id: str,
    journal_entry_id: Optional[str],
    invoice_id: Optional[str],
) -> TransactionInDB:
    node = await _get_node(session, user_id, "BankFeedTransaction", "OWNS_BANK_TX", transaction_id)
    if not node:
        raise NotFoundError("Transaction not found")
    set_lines = (
        "x.linked_journal_entry_id = $journal_entry_id,\n        "
        "x.linked_invoice_id = $invoice_id,\n        "
        "x.status = $status,\n        "
        "x.updated_at = datetime($updated_at)"
    )
    node = await _set_node_props(
        session,
        user_id,
        "BankFeedTransaction",
        "OWNS_BANK_TX",
        transaction_id,
        set_lines,
        {
            "journal_entry_id": journal_entry_id,
            "invoice_id": invoice_id,
            "status": TransactionStatus.RECONCILED.value,
            "updated_at": _now().isoformat(),
        },
    )
    return _tx_from_node(node)


# --- reconciliation rules ---


async def create_rule(session: AsyncSession, user_id: str, rule: ReconciliationRule) -> ReconciliationRule:
    rule_id = str(uuid.uuid4())
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:BankReconciliationRule {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        description: $description,
        match_conditions_json: $match_conditions_json,
        priority: toInteger($priority),
        auto_match_enabled: $auto_match_enabled,
        create_journal_entry: $create_journal_entry,
        journal_entry_template_json: $journal_entry_template_json,
        active: $active
    })
    CREATE (u)-[:OWNS_BANK_RULE]->(x)
    RETURN x
    """
    params = {
        "id": rule_id,
        "name": rule.name,
        "description": rule.description,
        "match_conditions_json": json.dumps(rule.match_conditions),
        "priority": rule.priority,
        "auto_match_enabled": rule.auto_match_enabled,
        "create_journal_entry": rule.create_journal_entry,
        "journal_entry_template_json": json.dumps(rule.journal_entry_template),
        "active": rule.active,
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [r async for r in result]
    return _rule_from_node(dict(records[0]["x"]))


async def list_rules(session: AsyncSession, user_id: str, active_only: bool = False) -> List[ReconciliationRule]:
    rules = [
        _rule_from_node(n) for n in await _list_nodes(session, user_id, "BankReconciliationRule", "OWNS_BANK_RULE")
    ]
    if active_only:
        rules = [r for r in rules if r.active]
    return rules


async def update_rule(
    session: AsyncSession, user_id: str, rule_id: str, rule: ReconciliationRule
) -> ReconciliationRule:
    node = await _get_node(session, user_id, "BankReconciliationRule", "OWNS_BANK_RULE", rule_id)
    if not node:
        raise NotFoundError("Rule not found")
    set_lines = (
        "x.name = $name,\n        x.description = $description,\n        "
        "x.match_conditions_json = $match_conditions_json,\n        "
        "x.priority = toInteger($priority),\n        "
        "x.auto_match_enabled = $auto_match_enabled,\n        "
        "x.create_journal_entry = $create_journal_entry,\n        "
        "x.journal_entry_template_json = $journal_entry_template_json,\n        "
        "x.active = $active"
    )
    node = await _set_node_props(
        session,
        user_id,
        "BankReconciliationRule",
        "OWNS_BANK_RULE",
        rule_id,
        set_lines,
        {
            "name": rule.name,
            "description": rule.description,
            "match_conditions_json": json.dumps(rule.match_conditions),
            "priority": rule.priority,
            "auto_match_enabled": rule.auto_match_enabled,
            "create_journal_entry": rule.create_journal_entry,
            "journal_entry_template_json": json.dumps(rule.journal_entry_template),
            "active": rule.active,
        },
    )
    return _rule_from_node(node)


async def delete_rule(session: AsyncSession, user_id: str, rule_id: str) -> None:
    await _delete_node(session, user_id, "BankReconciliationRule", "OWNS_BANK_RULE", rule_id)


async def apply_rules_to_tx(session: AsyncSession, user_id: str, tx: TransactionInDB) -> Optional[str]:
    """Apply the caller's reconciliation rules to a transaction (original matching semantics)."""
    matched_rule_id = None
    for rule in sorted(await list_rules(session, user_id), key=lambda r: r.priority, reverse=True):
        if not rule.active or not rule.auto_match_enabled:
            continue
        conditions = rule.match_conditions
        match = True

        if "amount_min" in conditions or "amount_max" in conditions:
            amount = abs(tx.amount)
            if "amount_min" in conditions and amount < conditions["amount_min"]:
                match = False
            if "amount_max" in conditions and amount > conditions["amount_max"]:
                match = False
        if "merchant_pattern" in conditions and tx.merchant_name:
            if conditions["merchant_pattern"].lower() not in tx.merchant_name.lower():
                match = False
        if "categories" in conditions and tx.category:
            if tx.category not in conditions["categories"]:
                match = False
        if "transaction_type" in conditions:
            if tx.transaction_type != conditions["transaction_type"]:
                match = False

        if match:
            matched_rule_id = rule.id
            set_lines = (
                "x.matched_rule_id = $matched_rule_id,\n        "
                "x.confidence_score = toFloat($confidence_score),\n        "
                "x.updated_at = datetime($updated_at)"
            )
            await _set_node_props(
                session,
                user_id,
                "BankFeedTransaction",
                "OWNS_BANK_TX",
                tx.id,
                set_lines,
                {
                    "matched_rule_id": rule.id,
                    "confidence_score": 0.85,
                    "updated_at": _now().isoformat(),
                },
            )
            break
    return matched_rule_id


# --- sync history ---


async def create_sync(session: AsyncSession, user_id: str, sync_id: str, connection_id: str) -> SyncResult:
    now = _now()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:BankSync {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        bank_connection_id: $bank_connection_id,
        status: $status,
        transactions_imported: toInteger($transactions_imported),
        transactions_updated: toInteger($transactions_updated),
        transactions_matched: toInteger($transactions_matched),
        errors_json: $errors_json,
        started_at: datetime($started_at)
    })
    CREATE (u)-[:OWNS_BANK_SYNC]->(x)
    RETURN x
    """
    params = {
        "id": sync_id,
        "bank_connection_id": connection_id,
        "status": SyncStatus.SYNCING.value,
        "transactions_imported": 0,
        "transactions_updated": 0,
        "transactions_matched": 0,
        "errors_json": json.dumps([]),
        "started_at": now.isoformat(),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [r async for r in result]
    return _sync_from_node(dict(records[0]["x"]))


async def update_sync(
    session: AsyncSession,
    user_id: str,
    sync_id: str,
    status: SyncStatus,
    imported: Optional[int] = None,
    updated: Optional[int] = None,
    matched: Optional[int] = None,
    errors: Optional[List[str]] = None,
    completed: Optional[datetime] = None,
) -> SyncResult:
    node = await _get_node(session, user_id, "BankSync", "OWNS_BANK_SYNC", sync_id)
    if not node:
        raise NotFoundError("Sync not found")
    current = _sync_from_node(node)
    set_lines = (
        "x.status = $status,\n        "
        "x.transactions_imported = toInteger($transactions_imported),\n        "
        "x.transactions_updated = toInteger($transactions_updated),\n        "
        "x.transactions_matched = toInteger($transactions_matched),\n        "
        "x.errors_json = $errors_json,\n        "
        "x.completed_at = datetime($completed_at)"
    )
    node = await _set_node_props(
        session,
        user_id,
        "BankSync",
        "OWNS_BANK_SYNC",
        sync_id,
        set_lines,
        {
            "status": status.value,
            "transactions_imported": current.transactions_imported if imported is None else imported,
            "transactions_updated": current.transactions_updated if updated is None else updated,
            "transactions_matched": current.transactions_matched if matched is None else matched,
            "errors_json": json.dumps(errors if errors is not None else current.errors),
            "completed_at": completed.isoformat() if completed else None,
        },
    )
    return _sync_from_node(node)


async def get_sync(session: AsyncSession, user_id: str, sync_id: str) -> SyncResult:
    node = await _get_node(session, user_id, "BankSync", "OWNS_BANK_SYNC", sync_id)
    if not node:
        raise NotFoundError("Sync not found")
    return _sync_from_node(node)


async def list_syncs(
    session: AsyncSession, user_id: str, connection_id: Optional[str] = None, limit: int = 50
) -> Dict[str, Any]:
    syncs = [_sync_from_node(n) for n in await _list_nodes(session, user_id, "BankSync", "OWNS_BANK_SYNC")]
    if connection_id:
        syncs = [s for s in syncs if s.bank_connection_id == connection_id]
    syncs.sort(key=lambda x: x.started_at, reverse=True)
    return {"total": len(syncs), "history": syncs[:limit]}


# --- statistics ---


async def statistics(session: AsyncSession, user_id: str, organization_id: str) -> Dict[str, Any]:
    conns = [
        c
        for c in [_conn_from_node(n) for n in await _list_nodes(session, user_id, "BankConnection", "OWNS_CONNECTION")]
        if c.organization_id == organization_id
    ]
    txs = await _list_txs(session, user_id)
    conn_txs = {c.id: [t for t in txs if t.bank_connection_id == c.id] for c in conns}

    total_transactions = sum(len(v) for v in conn_txs.values())
    reconciled = sum(1 for v in conn_txs.values() for t in v if t.status == TransactionStatus.RECONCILED)
    pending = sum(1 for v in conn_txs.values() for t in v if t.status == TransactionStatus.PENDING)

    by_provider = {}
    for c in conns:
        provider = c.provider.value
        by_provider.setdefault(provider, {"connections": 0, "transactions": 0})
        by_provider[provider]["connections"] += 1
        by_provider[provider]["transactions"] += len(conn_txs[c.id])

    return {
        "total_connections": len(conns),
        "total_transactions": total_transactions,
        "reconciled": reconciled,
        "pending": pending,
        "reconciliation_rate": round(reconciled / total_transactions * 100, 2) if total_transactions else 0,
        "by_provider": by_provider,
    }
