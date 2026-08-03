"""
Vimbai POS Integration Service
Provides seamless integration with Point-of-Sale systems for real-time transaction syncing
"""

from fastapi import FastAPI, HTTPException, WebSocket, WebSocketDisconnect, Depends, status, BackgroundTasks
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field
from typing import List, Optional, Dict, Any
from datetime import datetime, timezone
from enum import Enum
import asyncio
import json
import uuid
import os
from dotenv import load_dotenv
import httpx

load_dotenv()

app = FastAPI(
    title="Vimbai POS Integration Service",
    description="Real-time POS transaction integration, inventory sync, and sales reconciliation",
    version="1.0.0",
)

# ============================================================================
# Models
# ============================================================================

class POSDeviceStatus(str, Enum):
    ONLINE = "online"
    OFFLINE = "offline"
    SYNCING = "syncing"
    ERROR = "error"

class TransactionType(str, Enum):
    SALE = "sale"
    REFUND = "refund"
    VOID = "void"
    ADJUSTMENT = "adjustment"
    LAYAWAY = "layaway"
    RETURN = "return"

class PaymentMethod(str, Enum):
    CASH = "cash"
    CARD = "card"
    MOBILE = "mobile"
    SPLIT = "split"
    GIFT_CARD = "gift_card"
    LOYALTY = "loyalty"

class SyncStatus(str, Enum):
    PENDING = "pending"
    SYNCED = "synced"
    FAILED = "failed"
    PARTIAL = "partial"

class POSDeviceCreate(BaseModel):
    device_id: str = Field(..., description="Unique POS device identifier")
    device_name: str = Field(..., min_length=3, max_length=100)
    device_type: str = Field(..., description="POS hardware type")
    location_id: Optional[str] = None
    api_key: Optional[str] = None
    webhook_url: Optional[str] = None
    enabled: bool = True

class POSDeviceInDB(POSDeviceCreate):
    id: str
    status: POSDeviceStatus = POSDeviceStatus.OFFLINE
    last_sync: Optional[datetime] = None
    created_at: datetime
    updated_at: datetime

class POSTransactionCreate(BaseModel):
    transaction_id: str = Field(..., description="External POS transaction ID")
    device_id: str
    transaction_type: TransactionType
    total_amount: float = Field(..., gt=0)
    tax_amount: float = 0
    discount_amount: float = 0
    payment_method: PaymentMethod
    payment_details: Optional[Dict[str, Any]] = None
    items: List[Dict[str, Any]] = Field(..., min_items=1)
    customer_id: Optional[str] = None
    employee_id: Optional[str] = None
    location_id: Optional[str] = None
    notes: Optional[str] = None
    timestamp: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))

class POSTransactionInDB(POSTransactionCreate):
    id: str
    sync_status: SyncStatus = SyncStatus.PENDING
    journal_entry_id: Optional[str] = None
    processed_at: Optional[datetime] = None
    error_message: Optional[str] = None
    created_at: datetime

class InventorySyncRequest(BaseModel):
    device_id: str
    products: List[Dict[str, Any]] = Field(..., description="Product inventory updates")
    timestamp: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))

class SalesSummaryRequest(BaseModel):
    device_id: str
    start_date: datetime
    end_date: datetime
    group_by: str = "hour"  # hour, day, week

# ============================================================================
# Connection Manager
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

# ============================================================================
# In-Memory Storage (Use Neo4j in production)
# ============================================================================

devices: Dict[str, POSDeviceInDB] = {}
transactions: Dict[str, POSTransactionInDB] = {}
transaction_mappings: Dict[str, str] = {}  # external_id -> internal_id

# ============================================================================
# API Endpoints
# ============================================================================

@app.get("/")
async def health_check():
    return {
        "status": "healthy",
        "service": "pos-integration",
        "connected_devices": len(pos_manager.active_connections),
        "total_transactions": len(transactions)
    }

# --- Device Management ---
@app.post("/devices", response_model=POSDeviceInDB, status_code=status.HTTP_201_CREATED)
async def register_device(device: POSDeviceCreate):
    """Register a new POS device"""
    device_id = device.device_id

    if device_id in devices:
        raise HTTPException(status_code=status.HTTP_409_CONFLICT, detail="Device already registered")

    now = datetime.now(timezone.utc)
    db_device = POSDeviceInDB(
        id=str(uuid.uuid4()),
        **device.model_dump(),
        status=POSDeviceStatus.OFFLINE,
        created_at=now,
        updated_at=now
    )

    devices[device_id] = db_device
    return db_device

@app.get("/devices", response_model=List[POSDeviceInDB])
async def list_devices(status: Optional[POSDeviceStatus] = None):
    """List all registered POS devices"""
    devices_list = list(devices.values())
    if status:
        devices_list = [d for d in devices_list if d.status == status]
    return devices_list

@app.get("/devices/{device_id}", response_model=POSDeviceInDB)
async def get_device(device_id: str):
    if device_id not in devices:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Device not found")
    return devices[device_id]

@app.put("/devices/{device_id}/status")
async def update_device_status(device_id: str, status_update: Dict[str, Any]):
    """Update device status"""
    if device_id not in devices:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Device not found")

    devices[device_id].status = POSDeviceStatus(status_update.get("status", "online"))
    devices[device_id].last_sync = datetime.now(timezone.utc)
    devices[device_id].updated_at = datetime.now(timezone.utc)

    await pos_manager.broadcast_to_all({
        "type": "device_status_update",
        "device_id": device_id,
        "status": devices[device_id].status.value
    })

    return {"status": "updated", "device_id": device_id}

# --- Transaction Processing ---
@app.post("/transactions", response_model=POSTransactionInDB, status_code=status.HTTP_201_CREATED)
async def receive_transaction(
    transaction: POSTransactionCreate,
    background_tasks: BackgroundTasks
):
    """Receive transaction from POS device and process for accounting"""
    transaction_id = transaction.transaction_id

    if transaction_id in transaction_mappings:
        raise HTTPException(status_code=status.HTTP_409_CONFLICT, detail="Transaction already received")

    now = datetime.now(timezone.utc)
    db_transaction = POSTransactionInDB(
        id=str(uuid.uuid4()),
        **transaction.model_dump(),
        sync_status=SyncStatus.PENDING,
        created_at=now
    )

    transactions[transaction_id] = db_transaction
    transaction_mappings[transaction_id] = db_transaction.id

    # Process in background - create journal entry
    background_tasks.add_task(process_transaction_for_accounting, db_transaction)

    # Broadcast to connected dashboards
    await pos_manager.broadcast_to_all({
        "type": "new_transaction",
        "transaction": {
            "id": db_transaction.id,
            "external_id": transaction_id,
            "amount": db_transaction.total_amount,
            "type": db_transaction.transaction_type.value
        }
    })

    return db_transaction

async def process_transaction_for_accounting(transaction: POSTransactionInDB):
    """Process POS transaction and create journal entry"""
    try:
        # Simulate calling accounting service
        # In production, this would call the accounting service via message queue

        # Create journal entry data
        journal_entry_data = {
            "description": f"POS Transaction {transaction.transaction_id}",
            "entry_date": transaction.timestamp.isoformat(),
            "reference": f"POS-{transaction.device_id}-{transaction.transaction_id}",
            "lines": []
        }

        # For sales, create debit to cash/receivables and credit to sales
        if transaction.transaction_type == TransactionType.SALE:
            # Debit entry (cash or accounts receivable)
            journal_entry_data["lines"].append({
                "account_code": "1100",  # Cash account
                "description": "Cash from POS sale",
                "debit": transaction.total_amount - transaction.tax_amount,
                "credit": 0
            })
            # Tax liability
            if transaction.tax_amount > 0:
                journal_entry_data["lines"].append({
                    "account_code": "2200",  # Sales Tax Payable
                    "description": "Sales tax collected",
                    "debit": 0,
                    "credit": transaction.tax_amount
                })
            # Credit to sales revenue
            journal_entry_data["lines"].append({
                "account_code": "4000",  # Sales Revenue
                "description": "POS Sale",
                "debit": 0,
                "credit": transaction.total_amount - transaction.tax_amount
            })

        # Update transaction status
        transaction.sync_status = SyncStatus.SYNCED
        transaction.processed_at = datetime.now(timezone.utc)

        # Broadcast update
        await pos_manager.broadcast_to_device(transaction.device_id, {
            "type": "transaction_synced",
            "transaction_id": transaction.transaction_id,
            "journal_entry_id": f"JE-{transaction.id[:8]}"
        })

    except Exception as e:
        transaction.sync_status = SyncStatus.FAILED
        transaction.error_message = str(e)

@app.post("/transactions/batch", status_code=status.HTTP_201_CREATED)
async def receive_batch_transactions(
    transactions_list: List[POSTransactionCreate],
    background_tasks: BackgroundTasks
):
    """Receive multiple transactions from POS device"""
    results = []

    for transaction in transactions_list:
        try:
            db_transaction = await receive_transaction(transaction, background_tasks)
            results.append({
                "transaction_id": transaction.transaction_id,
                "status": "accepted",
                "internal_id": db_transaction.id
            })
        except HTTPException as e:
            results.append({
                "transaction_id": transaction.transaction_id,
                "status": "rejected",
                "reason": e.detail
            })

    return {"total": len(transactions_list), "results": results}

@app.get("/transactions", response_model=List[POSTransactionInDB])
async def list_transactions(
    device_id: Optional[str] = None,
    status: Optional[SyncStatus] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100
):
    """List POS transactions with filters"""
    filtered = list(transactions.values())

    if device_id:
        filtered = [t for t in filtered if t.device_id == device_id]
    if status:
        filtered = [t for t in filtered if t.sync_status == status]
    if start_date:
        filtered = [t for t in filtered if t.timestamp >= start_date]
    if end_date:
        filtered = [t for t in filtered if t.timestamp <= end_date]

    return sorted(filtered, key=lambda x: x.created_at, reverse=True)[:limit]

@app.get("/transactions/{transaction_id}", response_model=POSTransactionInDB)
async def get_transaction(transaction_id: str):
    if transaction_id not in transactions:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Transaction not found")
    return transactions[transaction_id]

# --- Inventory Sync ---
@app.post("/inventory/sync")
async def sync_inventory(request: InventorySyncRequest):
    """Sync inventory from POS to central system"""
    # In production, this would update the supply chain service

    return {
        "status": "synced",
        "device_id": request.device_id,
        "items_updated": len(request.products),
        "timestamp": datetime.now(timezone.utc).isoformat()
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
        "discrepancies": discrepancies
    }

# --- Sales Summary ---
@app.post("/reports/sales-summary")
async def get_sales_summary(request: SalesSummaryRequest):
    """Generate sales summary report for POS device"""
    filtered = [
        t for t in transactions.values()
        if t.device_id == request.device_id
        and request.start_date <= t.timestamp <= request.end_date
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
            "void": sum(1 for t in filtered if t.transaction_type == TransactionType.VOID)
        }
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
                    await websocket.send_json({"type": "pong", "timestamp": datetime.now(timezone.utc).isoformat()})
                elif message.get("type") == "status_update":
                    # Update device status
                    if device_id in devices:
                        devices[device_id].status = POSDeviceStatus(message.get("status", "online"))
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
async def receive_pos_webhook(pos_type: str, payload: Dict[str, Any]):
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

    # Process the transaction
    db_transaction = POSTransactionInDB(
        id=str(uuid.uuid4()),
        **transaction.model_dump(),
        sync_status=SyncStatus.PENDING,
        created_at=datetime.now(timezone.utc)
    )

    transactions[transaction.transaction_id] = db_transaction
    transaction_mappings[transaction.transaction_id] = db_transaction.id

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
        "timestamp": datetime.now(timezone.utc)
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
        "timestamp": datetime.fromtimestamp(payload.get("created", 0), tz=timezone.utc)
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
        "timestamp": datetime.now(timezone.utc)
    }

# --- Health and Metrics ---
@app.get("/metrics")
async def get_metrics():
    """Get POS integration metrics"""
    total_transactions = len(transactions)
    synced = sum(1 for t in transactions.values() if t.sync_status == SyncStatus.SYNCED)
    pending = sum(1 for t in transactions.values() if t.sync_status == SyncStatus.PENDING)
    failed = sum(1 for t in transactions.values() if t.sync_status == SyncStatus.FAILED)

    total_amount = sum(t.total_amount for t in transactions.values())

    return {
        "total_transactions": total_transactions,
        "synced": synced,
        "pending": pending,
        "failed": failed,
        "total_amount_processed": total_amount,
        "connected_devices": len(pos_manager.active_connections),
        "registered_devices": len(devices)
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8095)