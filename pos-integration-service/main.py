"""
Vimbai POS Integration Service
Provides seamless integration with Point-of-Sale systems for real-time
transaction syncing. Devices and transactions persist in Neo4j,
caller-owned (X-User-Id) and Book-gated (X-Book-ID, verified upstream by
the API gateway). Live WebSocket connections stay in the ephemeral
connection manager by design.

This file may be imported bare (uvicorn main:app), so it bootstraps its
own package alias before importing sibling modules.
"""

import asyncio
import importlib.util
import json
import os as _os
import sys as _sys
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from fastapi import (
    BackgroundTasks,
    Depends,
    FastAPI,
    HTTPException,
    Request,
    WebSocket,
    WebSocketDisconnect,
    status,
)
from fastapi.responses import JSONResponse
from neo4j import AsyncSession

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "pos_integration_service" not in _sys.modules or not hasattr(
    _sys.modules.get("pos_integration_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("pos_integration_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["pos_integration_service"] = _pkg
    _sys.modules["pos_integration_service"].__path__ = [_HERE]

from pos_integration_service import crud
from pos_integration_service.dependencies import book_id_var, get_db_session, get_user_id
from pos_integration_service.models import (
    InventorySyncRequest,
    PaymentMethod,
    POSDeviceCreate,
    POSDeviceInDB,
    POSDeviceStatus,
    POSTransactionCreate,
    POSTransactionInDB,
    SalesSummaryRequest,
    SyncStatus,
    TransactionType,
)

app = FastAPI(
    title="Vimbai POS Integration Service",
    description="Real-time POS transaction integration, inventory sync, and sales reconciliation",
    version="1.0.0",
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


# ============================================================================
# Connection Manager (live sockets only — ephemeral by design)
# ============================================================================


class POSConnectionManager:
    """Manages WebSocket connections for real-time POS updates"""

    def __init__(self):
        self.active_connections: Dict[str, List[WebSocket]] = {}
        self.device_status: Dict[str, POSDeviceStatus] = {}
        self.lock = asyncio.Lock()

    async def connect(self, websocket: WebSocket, device_id: str):
        await websocket.accept()
        async with self.lock:
            if device_id not in self.active_connections:
                self.active_connections[device_id] = []
            self.active_connections[device_id].append(websocket)
            self.device_status[device_id] = POSDeviceStatus.ONLINE

    async def disconnect(self, websocket: WebSocket, device_id: str):
        async with self.lock:
            if device_id in self.active_connections:
                try:
                    self.active_connections[device_id].remove(websocket)
                except ValueError:
                    pass
                if not self.active_connections[device_id]:
                    del self.active_connections[device_id]
                    self.device_status[device_id] = POSDeviceStatus.OFFLINE

    async def broadcast_to_device(self, device_id: str, message: dict):
        if device_id in self.active_connections:
            for connection in self.active_connections[device_id]:
                try:
                    await connection.send_json(message)
                except Exception:
                    pass

    async def broadcast_to_all(self, message: dict):
        for device_id, connections in self.active_connections.items():
            for connection in connections:
                try:
                    await connection.send_json(message)
                except Exception:
                    pass


pos_manager = POSConnectionManager()


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


# ============================================================================
# API Endpoints
# ============================================================================


@app.get("/")
async def health_check(
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    transactions = await crud.list_transactions(db_session, caller_id)
    return {
        "status": "healthy",
        "service": "pos-integration",
        "connected_devices": len(pos_manager.active_connections),
        "total_transactions": len(transactions),
    }


# --- Device Management ---


@app.post("/devices", response_model=POSDeviceInDB, status_code=status.HTTP_201_CREATED)
async def register_device(
    device: POSDeviceCreate,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Register a new POS device"""
    if await crud.get_device_by_external_id(db_session, caller_id, device.device_id):
        raise HTTPException(status_code=status.HTTP_409_CONFLICT, detail="Device already registered")

    now = _utcnow()
    db_device = POSDeviceInDB(
        id=str(uuid.uuid4()), **device.model_dump(), status=POSDeviceStatus.OFFLINE, created_at=now, updated_at=now
    )
    return await crud.create_device(db_session, caller_id, db_device)


@app.get("/devices", response_model=List[POSDeviceInDB])
async def list_devices(
    status: Optional[POSDeviceStatus] = None,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all registered POS devices (caller's own Book-visible devices)"""
    devices_list = await crud.list_devices(db_session, caller_id)
    if status:
        devices_list = [d for d in devices_list if d.status == status]
    return devices_list


@app.get("/devices/{device_id}", response_model=POSDeviceInDB)
async def get_device(
    device_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    device = await crud.get_device_by_external_id(db_session, caller_id, device_id)
    if not device:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Device not found")
    return device


@app.put("/devices/{device_id}/status")
async def update_device_status(
    device_id: str,
    status_update: Dict[str, Any],
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update device status (caller's own devices only)"""
    if not await crud.get_device_by_external_id(db_session, caller_id, device_id):
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Device not found")

    now = _utcnow()
    updated = await crud.set_device_status(
        db_session, caller_id, device_id, status_update.get("status", "online"), now, now
    )

    await pos_manager.broadcast_to_all(
        {"type": "device_status_update", "device_id": device_id, "status": updated.status.value}
    )

    return {"status": "updated", "device_id": device_id}


# --- Transaction Processing ---


async def _ingest_transaction(
    transaction: POSTransactionCreate,
    caller_id: str,
    db_session: AsyncSession,
) -> POSTransactionInDB:
    """Validate and persist an incoming transaction (caller-scoped duplicate check)."""
    if await crud.get_transaction_by_external_id(db_session, caller_id, transaction.transaction_id):
        raise HTTPException(status_code=status.HTTP_409_CONFLICT, detail="Transaction already received")

    db_transaction = POSTransactionInDB(
        id=str(uuid.uuid4()),
        **transaction.model_dump(),
        sync_status=SyncStatus.PENDING,
        created_at=_utcnow(),
    )
    saved = await crud.create_transaction(db_session, caller_id, db_transaction)

    # Broadcast to connected dashboards
    await pos_manager.broadcast_to_all(
        {
            "type": "new_transaction",
            "transaction": {
                "id": saved.id,
                "external_id": saved.transaction_id,
                "amount": saved.total_amount,
                "type": saved.transaction_type.value,
            },
        }
    )
    return saved


@app.post("/transactions", response_model=POSTransactionInDB, status_code=status.HTTP_201_CREATED)
async def receive_transaction(
    transaction: POSTransactionCreate,
    background_tasks: BackgroundTasks,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Receive transaction from POS device and process for accounting"""
    saved = await _ingest_transaction(transaction, caller_id, db_session)

    # Process in background - create journal entry
    background_tasks.add_task(process_transaction_for_accounting, saved.id, caller_id)

    return saved


async def process_transaction_for_accounting(transaction_id: str, caller_id: str):
    """Process POS transaction and create journal entry (caller-scoped persistence)"""
    from pos_integration_service.database import Neo4jConnector

    async with Neo4jConnector.get_driver().session() as session:
        try:
            db_transaction = await crud.get_transaction_by_id(session, caller_id, transaction_id)
            if not db_transaction:
                return

            # Simulate calling accounting service
            # In production, this would call the accounting service via message queue

            # Update transaction status
            await crud.mark_transaction_synced(
                session, caller_id, transaction_id, f"JE-{transaction_id[:8]}", _utcnow()
            )

            # Broadcast update
            await pos_manager.broadcast_to_device(
                db_transaction.device_id,
                {
                    "type": "transaction_synced",
                    "transaction_id": db_transaction.transaction_id,
                    "journal_entry_id": f"JE-{transaction_id[:8]}",
                },
            )

        except Exception as e:
            await crud.mark_transaction_failed(session, caller_id, transaction_id, str(e), _utcnow())


@app.post("/transactions/batch", status_code=status.HTTP_201_CREATED)
async def receive_batch_transactions(
    transactions_list: List[POSTransactionCreate],
    background_tasks: BackgroundTasks,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Receive multiple transactions from POS device"""
    results = []

    for transaction in transactions_list:
        try:
            db_transaction = await _ingest_transaction(transaction, caller_id, db_session)
            background_tasks.add_task(process_transaction_for_accounting, db_transaction.id, caller_id)
            results.append(
                {"transaction_id": transaction.transaction_id, "status": "accepted", "internal_id": db_transaction.id}
            )
        except HTTPException as e:
            results.append({"transaction_id": transaction.transaction_id, "status": "rejected", "reason": e.detail})

    return {"total": len(transactions_list), "results": results}


@app.get("/transactions", response_model=List[POSTransactionInDB])
async def list_transactions(
    device_id: Optional[str] = None,
    sync_status: Optional[SyncStatus] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List POS transactions with filters (caller's own Book-visible set)"""
    filtered = await crud.list_transactions(db_session, caller_id)

    if device_id:
        filtered = [t for t in filtered if t.device_id == device_id]
    if sync_status:
        filtered = [t for t in filtered if t.sync_status == sync_status]
    if start_date:
        filtered = [t for t in filtered if t.timestamp >= start_date]
    if end_date:
        filtered = [t for t in filtered if t.timestamp <= end_date]

    return sorted(filtered, key=lambda x: x.created_at, reverse=True)[:limit]


@app.get("/transactions/{transaction_id}", response_model=POSTransactionInDB)
async def get_transaction(
    transaction_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    t = await crud.get_transaction_by_external_id(db_session, caller_id, transaction_id)
    if not t:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Transaction not found")
    return t


# --- Inventory Sync ---


@app.post("/inventory/sync")
async def sync_inventory(request: InventorySyncRequest):
    """Sync inventory from POS to central system"""
    # In production, this would update the supply chain service

    return {
        "status": "synced",
        "device_id": request.device_id,
        "items_updated": len(request.products),
        "timestamp": _utcnow().isoformat(),
    }


@app.post("/inventory/reconcile")
async def reconcile_inventory(device_id: str, inventory_data: List[Dict[str, Any]]):
    """Reconcile POS inventory with central system"""
    # Find discrepancies
    discrepancies = []

    for item in inventory_data:
        # Compare with expected quantities
        pass  # Implementation here

    return {
        "device_id": device_id,
        "total_items": len(inventory_data),
        "discrepancies_found": len(discrepancies),
        "discrepancies": discrepancies,
    }


# --- Sales Summary ---


@app.post("/reports/sales-summary")
async def get_sales_summary(
    request: SalesSummaryRequest,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Generate sales summary report for POS device (caller's transactions only)"""
    all_transactions = await crud.list_transactions(db_session, caller_id)
    filtered = [
        t
        for t in all_transactions
        if t.device_id == request.device_id and request.start_date <= t.timestamp <= request.end_date
    ]

    total_sales = sum(t.total_amount for t in filtered if t.transaction_type == TransactionType.SALE)
    total_refunds = sum(t.total_amount for t in filtered if t.transaction_type == TransactionType.REFUND)
    total_voids = sum(t.total_amount for t in filtered if t.transaction_type == TransactionType.VOID)

    by_payment = {}
    for t in filtered:
        method = t.payment_method.value
        by_payment[method] = by_payment.get(method, 0) + t.total_amount

    return {
        "device_id": request.device_id,
        "start_date": request.start_date.isoformat(),
        "end_date": request.end_date.isoformat(),
        "group_by": request.group_by,
        "total_transactions": len(filtered),
        "total_sales": total_sales,
        "total_refunds": total_refunds,
        "total_voids": total_voids,
        "net_sales": total_sales - total_refunds - total_voids,
        "by_payment_method": by_payment,
        "by_type": {
            "sale": sum(1 for t in filtered if t.transaction_type == TransactionType.SALE),
            "refund": sum(1 for t in filtered if t.transaction_type == TransactionType.REFUND),
            "void": sum(1 for t in filtered if t.transaction_type == TransactionType.VOID),
        },
    }


# --- WebSocket for Real-time Updates ---


@app.websocket("/ws/pos/{device_id}")
async def websocket_pos(websocket: WebSocket, device_id: str):
    """WebSocket endpoint for real-time POS updates"""
    await pos_manager.connect(websocket, device_id)
    try:
        while True:
            data = await websocket.receive_text()
            try:
                message = json.loads(data)
                if message.get("type") == "ping":
                    await websocket.send_json({"type": "pong", "timestamp": _utcnow().isoformat()})
                elif message.get("type") == "status_update":
                    # Persist the device status change for the socket's caller
                    caller_id = websocket.headers.get("X-User-Id")
                    if caller_id and device_id:
                        from pos_integration_service.database import Neo4jConnector

                        async with Neo4jConnector.get_driver().session() as session:
                            if await crud.get_device_by_external_id(session, caller_id, device_id):
                                now = _utcnow()
                                await crud.set_device_status(
                                    session,
                                    caller_id,
                                    device_id,
                                    message.get("status", "online"),
                                    now,
                                    now,
                                )
            except json.JSONDecodeError:
                await websocket.send_json({"type": "error", "message": "Invalid JSON"})
    except WebSocketDisconnect:
        await pos_manager.disconnect(websocket, device_id)


@app.websocket("/ws/dashboard")
async def websocket_dashboard(websocket: WebSocket):
    """WebSocket endpoint for dashboard to receive all POS updates"""
    await websocket.accept()
    try:
        while True:
            data = await websocket.receive_text()
            # Dashboard sends ping
            if data == "ping":
                await websocket.send_json({"type": "pong"})
    except WebSocketDisconnect:
        pass


# --- External Integration Endpoints ---


@app.post("/integrations/{pos_type}/webhook")
async def receive_pos_webhook(
    pos_type: str,
    payload: Dict[str, Any],
    background_tasks: BackgroundTasks,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Receive transaction data from external POS systems"""
    # Support for Square, Stripe, Shopify, etc.

    if pos_type == "square":
        transaction_data = transform_square_transaction(payload)
    elif pos_type == "stripe":
        transaction_data = transform_stripe_transaction(payload)
    elif pos_type == "shopify":
        transaction_data = transform_shopify_transaction(payload)
    else:
        transaction_data = payload

    transaction = POSTransactionCreate(**transaction_data)

    # Process the transaction (caller-scoped duplicate check)
    saved = await _ingest_transaction(transaction, caller_id, db_session)
    background_tasks.add_task(process_transaction_for_accounting, saved.id, caller_id)

    return {"status": "received", "transaction_id": transaction.transaction_id}


def transform_square_transaction(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Transform Square webhook payload to standard format"""
    return {
        "transaction_id": payload.get("id", str(uuid.uuid4())),
        "device_id": payload.get("location_id", "unknown"),
        "transaction_type": TransactionType.SALE,
        "total_amount": float(payload.get("total_money", {}).get("amount", 0)) / 100,
        "tax_amount": float(payload.get("tax_money", {}).get("amount", 0)) / 100,
        "discount_amount": 0,
        "payment_method": PaymentMethod.CARD,
        "items": [],
        "timestamp": _utcnow(),
    }


def transform_stripe_transaction(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Transform Stripe webhook payload to standard format"""
    return {
        "transaction_id": payload.get("id", str(uuid.uuid4())),
        "device_id": payload.get("metadata", {}).get("device_id", "unknown"),
        "transaction_type": TransactionType.SALE,
        "total_amount": float(payload.get("amount", 0)) / 100,
        "tax_amount": 0,
        "discount_amount": 0,
        "payment_method": PaymentMethod.CARD,
        "items": [],
        "timestamp": datetime.fromtimestamp(payload.get("created", 0), tz=timezone.utc),
    }


def transform_shopify_transaction(payload: Dict[str, Any]) -> Dict[str, Any]:
    """Transform Shopify webhook payload to standard format"""
    return {
        "transaction_id": str(payload.get("id", "")),
        "device_id": payload.get("gateway", "online"),
        "transaction_type": TransactionType.SALE,
        "total_amount": float(payload.get("total_price", 0)),
        "tax_amount": float(payload.get("total_tax", 0)),
        "discount_amount": float(payload.get("total_discounts", 0)),
        "payment_method": PaymentMethod.CARD,
        "items": [],
        "timestamp": _utcnow(),
    }


# --- Health and Metrics ---


@app.get("/metrics")
async def get_metrics(
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get POS integration metrics (over the caller's Book-visible data)"""
    transactions = await crud.list_transactions(db_session, caller_id)
    synced = sum(1 for t in transactions if t.sync_status == SyncStatus.SYNCED)
    pending = sum(1 for t in transactions if t.sync_status == SyncStatus.PENDING)
    failed = sum(1 for t in transactions if t.sync_status == SyncStatus.FAILED)

    total_amount = sum(t.total_amount for t in transactions)
    devices = await crud.list_devices(db_session, caller_id)

    return {
        "total_transactions": len(transactions),
        "synced": synced,
        "pending": pending,
        "failed": failed,
        "total_amount_processed": total_amount,
        "connected_devices": len(pos_manager.active_connections),
        "registered_devices": len(devices),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=int(_os.getenv("PORT", "8095")))
