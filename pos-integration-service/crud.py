"""
POS Integration Service CRUD Operations

Devices and transactions persist in Neo4j, caller-owned (X-User-Id) and
Book-gated (X-Book-ID, verified upstream by the API gateway). Previously
the three shared module-level stores let any caller read/reregister anyone's
POS devices and ingest/duplicate transactions against them. The WebSocket
connection registry stays ephemeral by design (live sockets cannot persist).
"""

from typing import Dict, List, Optional

from neo4j import AsyncSession
from pos_integration_service.dependencies import book_id_var
from pos_integration_service.models import POSDeviceInDB, POSDeviceStatus, POSTransactionInDB, SyncStatus

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
        from datetime import datetime

        dt = datetime.fromisoformat(iso)
        return dt if dt.tzinfo else dt.replace(tzinfo=__import__("datetime").timezone.utc)
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


# --- devices ---


def _device_from_node(n: Dict) -> POSDeviceInDB:
    return POSDeviceInDB(
        id=n["id"],
        device_id=n.get("device_id", ""),
        device_name=n.get("device_name", ""),
        device_type=n.get("device_type", ""),
        location_id=n.get("location_id"),
        api_key=n.get("api_key"),
        webhook_url=n.get("webhook_url"),
        enabled=bool(n.get("enabled", True)),
        status=POSDeviceStatus(n.get("status", "offline")),
        last_sync=_as_dt(n.get("last_sync")),
        created_at=_as_dt(n.get("created_at")),
        updated_at=_as_dt(n.get("updated_at")),
    )


def _device_props(d: POSDeviceInDB) -> Dict:
    return {
        "id": d.id,
        "device_id": d.device_id,
        "device_name": d.device_name,
        "device_type": d.device_type,
        "location_id": d.location_id,
        "api_key": d.api_key,
        "webhook_url": d.webhook_url,
        "enabled": bool(d.enabled),
        "status": d.status.value if hasattr(d.status, "value") else str(d.status),
        "last_sync": _iso(d.last_sync),
        "created_at": _iso(d.created_at),
        "updated_at": _iso(d.updated_at),
    }


async def create_device(session: AsyncSession, user_id: str, d: POSDeviceInDB) -> POSDeviceInDB:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:POSDevice {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        device_id: $device_id,
        device_name: $device_name,
        device_type: $device_type,
        location_id: $location_id,
        api_key: $api_key,
        webhook_url: $webhook_url,
        enabled: $enabled,
        status: $status,
        last_sync: datetime($last_sync),
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_DEVICE]->(x)
    RETURN x
    """
    result = await _run(session, query, _device_props(d), user_id=user_id)
    rec = await result.single()
    return _device_from_node(dict(rec["x"]))


async def get_device_by_external_id(session: AsyncSession, user_id: str, device_id: str) -> Optional[POSDeviceInDB]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DEVICE]->(x:POSDevice)
    WHERE x.device_id = $device_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, device_id=device_id, user_id=user_id)
    rec = await result.single()
    return _device_from_node(dict(rec["x"])) if rec else None


async def list_devices(session: AsyncSession, user_id: str) -> List[POSDeviceInDB]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DEVICE]->(x:POSDevice)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_device_from_node(dict(rec["x"])) async for rec in result]


async def set_device_status(
    session: AsyncSession, user_id: str, device_id: str, status_value: str, last_sync, updated_at
) -> Optional[POSDeviceInDB]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DEVICE]->(x:POSDevice)
    WHERE x.device_id = $device_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.status = $status,
        x.last_sync = datetime($last_sync),
        x.updated_at = datetime($updated_at)
    RETURN x
    """
    result = await _run(
        session,
        query,
        device_id=device_id,
        status=status_value,
        last_sync=_iso(last_sync),
        updated_at=_iso(updated_at),
        user_id=user_id,
    )
    rec = await result.single()
    return _device_from_node(dict(rec["x"])) if rec else None


# --- transactions ---


def _transaction_from_node(n: Dict) -> POSTransactionInDB:
    import json

    items = n.get("items", [])
    if isinstance(items, str):
        items = json.loads(items) if items else []
    payment_details = n.get("payment_details")
    if isinstance(payment_details, str):
        payment_details = json.loads(payment_details) if payment_details else None
    return POSTransactionInDB(
        id=n["id"],
        transaction_id=n.get("transaction_id", ""),
        device_id=n.get("device_id", ""),
        transaction_type=n.get("transaction_type", "sale"),
        total_amount=float(n.get("total_amount", 0.0)),
        tax_amount=float(n.get("tax_amount", 0.0)),
        discount_amount=float(n.get("discount_amount", 0.0)),
        payment_method=n.get("payment_method", "cash"),
        payment_details=payment_details,
        items=items,
        customer_id=n.get("customer_id"),
        employee_id=n.get("employee_id"),
        location_id=n.get("location_id"),
        notes=n.get("notes"),
        timestamp=_as_dt(n.get("timestamp")),
        sync_status=SyncStatus(n.get("sync_status", "pending")),
        journal_entry_id=n.get("journal_entry_id"),
        processed_at=_as_dt(n.get("processed_at")),
        error_message=n.get("error_message"),
        created_at=_as_dt(n.get("created_at")),
    )


def _transaction_props(t: POSTransactionInDB) -> Dict:
    import json

    return {
        "id": t.id,
        "transaction_id": t.transaction_id,
        "device_id": t.device_id,
        "transaction_type": (
            t.transaction_type.value if hasattr(t.transaction_type, "value") else str(t.transaction_type)
        ),
        "total_amount": float(t.total_amount),
        "tax_amount": float(t.tax_amount),
        "discount_amount": float(t.discount_amount),
        "payment_method": t.payment_method.value if hasattr(t.payment_method, "value") else str(t.payment_method),
        "payment_details": json.dumps(t.payment_details) if t.payment_details is not None else None,
        "items": json.dumps(t.items),
        "customer_id": t.customer_id,
        "employee_id": t.employee_id,
        "location_id": t.location_id,
        "notes": t.notes,
        "timestamp": _iso(t.timestamp),
        "sync_status": t.sync_status.value if hasattr(t.sync_status, "value") else str(t.sync_status),
        "journal_entry_id": t.journal_entry_id,
        "processed_at": _iso(t.processed_at),
        "error_message": t.error_message,
        "created_at": _iso(t.created_at),
    }


async def create_transaction(session: AsyncSession, user_id: str, t: POSTransactionInDB) -> POSTransactionInDB:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:POSTransaction {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        transaction_id: $transaction_id,
        device_id: $device_id,
        transaction_type: $transaction_type,
        total_amount: toFloat($total_amount),
        tax_amount: toFloat($tax_amount),
        discount_amount: toFloat($discount_amount),
        payment_method: $payment_method,
        payment_details: $payment_details,
        items: $items,
        customer_id: $customer_id,
        employee_id: $employee_id,
        location_id: $location_id,
        notes: $notes,
        timestamp: datetime($timestamp),
        sync_status: $sync_status,
        journal_entry_id: $journal_entry_id,
        processed_at: datetime($processed_at),
        error_message: $error_message,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_TRANSACTION]->(x)
    RETURN x
    """
    result = await _run(session, query, _transaction_props(t), user_id=user_id)
    rec = await result.single()
    return _transaction_from_node(dict(rec["x"]))


async def get_transaction_by_external_id(
    session: AsyncSession, user_id: str, transaction_id: str
) -> Optional[POSTransactionInDB]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_TRANSACTION]->(x:POSTransaction)
    WHERE x.transaction_id = $transaction_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, transaction_id=transaction_id, user_id=user_id)
    rec = await result.single()
    return _transaction_from_node(dict(rec["x"])) if rec else None


async def get_transaction_by_id(session: AsyncSession, user_id: str, internal_id: str) -> Optional[POSTransactionInDB]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_TRANSACTION]->(x:POSTransaction)
    WHERE x.id = $internal_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, internal_id=internal_id, user_id=user_id)
    rec = await result.single()
    return _transaction_from_node(dict(rec["x"])) if rec else None


async def list_transactions(session: AsyncSession, user_id: str) -> List[POSTransactionInDB]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_TRANSACTION]->(x:POSTransaction)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_transaction_from_node(dict(rec["x"])) async for rec in result]


async def mark_transaction_synced(
    session: AsyncSession, user_id: str, internal_id: str, journal_entry_id: str, processed_at
) -> None:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_TRANSACTION]->(x:POSTransaction)
    WHERE x.id = $internal_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.sync_status = $sync_status,
        x.journal_entry_id = $journal_entry_id,
        x.processed_at = datetime($processed_at)
    """
    await _run(
        session,
        query,
        internal_id=internal_id,
        sync_status=SyncStatus.SYNCED.value,
        journal_entry_id=journal_entry_id,
        processed_at=_iso(processed_at),
        user_id=user_id,
    )


async def mark_transaction_failed(
    session: AsyncSession, user_id: str, internal_id: str, error_message: str, processed_at
) -> None:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_TRANSACTION]->(x:POSTransaction)
    WHERE x.id = $internal_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.sync_status = $sync_status,
        x.error_message = $error_message,
        x.processed_at = datetime($processed_at)
    """
    await _run(
        session,
        query,
        internal_id=internal_id,
        sync_status=SyncStatus.FAILED.value,
        error_message=error_message,
        processed_at=_iso(processed_at),
        user_id=user_id,
    )
