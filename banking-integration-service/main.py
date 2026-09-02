"""
Vimbai Banking Integration Service
Handles bank connections, transaction sync, and reconciliation.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "banking-integration-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8386"))

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Banking Integration Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

# Distributed tracing
try:
    from shared.tracing import get_tracer, setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None
    import logging

    logging.getLogger(__name__).warning("OpenTelemetry not installed - tracing disabled")


class BankConnection(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    bank_name: str
    account_number: str
    account_type: str = "checking"
    api_key: str
    status: str = "active"
    last_sync: Optional[datetime] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class BankTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    connection_id: str
    amount: float
    transaction_type: str  # credit, debit
    description: str = ""
    transaction_date: datetime
    balance_after: float = 0.0
    reconciled: bool = False
    reference: str = ""


class ReconcileRequest(BaseModel):
    transaction_id: str
    matched_entry_id: Optional[str] = None
    notes: str = ""


connections: List[BankConnection] = []
transactions: List[BankTransaction] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/connect", response_model=BankConnection)
async def connect_bank(bank_name: str, account_number: str, api_key: str, account_type: str = "checking"):
    """Establish a connection to a bank account."""
    existing = next((c for c in connections if c.account_number == account_number and c.bank_name == bank_name), None)
    if existing:
        raise HTTPException(
            status_code=409, detail=f"Connection to {bank_name} account {account_number} already exists"
        )

    conn = BankConnection(
        bank_name=bank_name,
        account_number=account_number,
        api_key=api_key,
        account_type=account_type,
    )
    connections.append(conn)
    logger.info("Bank connection established", connection_id=conn.id, bank=bank_name)
    return conn


@app.get("/connections", response_model=List[BankConnection])
async def list_connections():
    """List all bank connections."""
    return connections


@app.post("/sync/{connection_id}")
async def sync_transactions(connection_id: str):
    """Sync transactions from a bank connection."""
    conn = next((c for c in connections if c.id == connection_id), None)
    if not conn:
        raise HTTPException(status_code=404, detail="Bank connection not found")
    if conn.status != "active":
        raise HTTPException(status_code=400, detail="Connection is not active")

    conn.last_sync = datetime.now(timezone.utc)
    logger.info("Transaction sync completed", connection_id=connection_id, bank=conn.bank_name)
    return {"connection_id": connection_id, "synced_at": conn.last_sync.isoformat(), "status": "success"}


@app.get("/transactions/{connection_id}", response_model=List[BankTransaction])
async def list_transactions(connection_id: str, limit: int = 50, offset: int = 0):
    """List transactions for a specific bank connection."""
    conn_txns = [t for t in transactions if t.connection_id == connection_id]
    return conn_txns[offset : offset + limit]


@app.post("/transactions/{connection_id}/reconcile")
async def reconcile_transaction(connection_id: str, request: ReconcileRequest):
    """Reconcile a bank transaction with an accounting entry."""
    txn = next((t for t in transactions if t.id == request.transaction_id and t.connection_id == connection_id), None)
    if not txn:
        raise HTTPException(status_code=404, detail="Transaction not found")

    txn.reconciled = True
    logger.info("Transaction reconciled", transaction_id=txn.id, matched_entry=request.matched_entry_id)
    return {"transaction_id": txn.id, "reconciled": True, "matched_entry": request.matched_entry_id}


@app.delete("/connections/{connection_id}")
async def disconnect_bank(connection_id: str):
    """Disconnect a bank connection."""
    global connections
    conn = next((c for c in connections if c.id == connection_id), None)
    if not conn:
        raise HTTPException(status_code=404, detail="Bank connection not found")

    conn.status = "disconnected"
    logger.info("Bank connection disconnected", connection_id=connection_id)
    return {"connection_id": connection_id, "status": "disconnected"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
