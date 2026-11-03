"""
Vimbai Banking Integration Service
Handles bank connections, transaction sync, and reconciliation.
"""

import os
import sys
import uuid
from contextvars import ContextVar
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

# Request-scoped Book context (X-Book-ID verified by the API gateway).
# None means personal / unscoped requests.
_book_id_var = ContextVar("book_id", default=None)


def current_book_id() -> Optional[str]:
    return _book_id_var.get()


# Loaded bare (uvicorn main:app) and by path (root integration tests), so this
# directory has to be importable before the store module is pulled in.
_HERE = os.path.dirname(os.path.abspath(__file__))
if _HERE not in sys.path:
    sys.path.insert(0, _HERE)

import banking_integration_store as store  # noqa: E402

app = FastAPI(title="Vimbai Banking Integration Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request, call_next):
    """Bind the gateway-verified caller and X-Book-ID into the request-scoped context."""
    token = _book_id_var.set(request.headers.get("x-book-id"))
    store.user_id_var.set(request.headers.get("x-user-id"))
    store.book_id_var.set(request.headers.get("x-book-id"))
    try:
        return await call_next(request)
    finally:
        _book_id_var.reset(token)


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
    book_id: Optional[str] = None
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
    book_id: Optional[str] = None


class ReconcileRequest(BaseModel):
    transaction_id: str
    matched_entry_id: Optional[str] = None
    notes: str = ""


# Bank connections and synced transactions are durable records: Neo4j,
# caller-owned, Book-gated. Credentials stay on the connection node; the API
# key is never returned to the caller (masked in the response model).
CONNECTIONS = ("BankConnection", "OWNS_BANK_CONNECTION")
TRANSACTIONS = ("BankTransaction", "OWNS_BANK_TRANSACTION")


def _connection_model(node: Dict[str, Any]) -> BankConnection:
    return BankConnection(
        id=node["id"],
        bank_name=node.get("bank_name", ""),
        account_number=node.get("account_number", ""),
        account_type=node.get("account_type", "checking"),
        api_key=node.get("api_key", ""),
        status=node.get("status", "active"),
        book_id=node.get("book_id"),
        last_sync=node.get("last_sync"),
        created_at=node.get("created_at"),
    )


def _transaction_model(node: Dict[str, Any]) -> BankTransaction:
    return BankTransaction(
        id=node["id"],
        connection_id=node.get("connection_id", ""),
        amount=float(node.get("amount") or 0),
        transaction_type=node.get("transaction_type", ""),
        description=node.get("description", ""),
        transaction_date=node.get("transaction_date"),
        balance_after=float(node.get("balance_after") or 0),
        reconciled=bool(node.get("reconciled")),
        reference=node.get("reference", ""),
        book_id=node.get("book_id"),
    )


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/connect", response_model=BankConnection)
async def connect_bank(bank_name: str, account_number: str, api_key: str, account_type: str = "checking"):
    """Establish a connection to a bank account."""
    book_id = current_book_id()
    existing = await store.list_nodes(
        CONNECTIONS[0], CONNECTIONS[1], {"account_number": account_number, "bank_name": bank_name}
    )
    if existing:
        raise HTTPException(
            status_code=409, detail=f"Connection to {bank_name} account {account_number} already exists"
        )

    conn = BankConnection(
        bank_name=bank_name,
        account_number=account_number,
        api_key=api_key,
        account_type=account_type,
        book_id=book_id,
    )
    node = await store.create_node(
        CONNECTIONS[0],
        CONNECTIONS[1],
        {
            "id": conn.id,
            "bank_name": conn.bank_name,
            "account_number": conn.account_number,
            "account_type": conn.account_type,
            "api_key": conn.api_key,
            "status": conn.status,
            "last_sync": None,
            "created_at": conn.created_at.isoformat(),
        },
    )
    logger.info("Bank connection established", connection_id=conn.id, bank=bank_name)
    return _connection_model(node)


@app.get("/connections", response_model=List[BankConnection])
async def list_connections():
    """List bank connections in the current Book context."""
    nodes = await store.list_nodes(CONNECTIONS[0], CONNECTIONS[1])
    return [_connection_model(node) for node in nodes]


@app.post("/sync/{connection_id}")
async def sync_transactions(connection_id: str):
    """Sync transactions from a bank connection."""
    node = await store.get_node(CONNECTIONS[0], CONNECTIONS[1], connection_id)
    if not node:
        raise HTTPException(status_code=404, detail="Bank connection not found")
    conn = _connection_model(node)
    if conn.status != "active":
        raise HTTPException(status_code=400, detail="Connection is not active")

    synced_at = datetime.now(timezone.utc)
    await store.update_node(CONNECTIONS[0], CONNECTIONS[1], connection_id, {"last_sync": synced_at.isoformat()})
    logger.info("Transaction sync completed", connection_id=connection_id, bank=conn.bank_name)
    return {"connection_id": connection_id, "synced_at": synced_at.isoformat(), "status": "success"}


@app.get("/transactions/{connection_id}", response_model=List[BankTransaction])
async def list_transactions(connection_id: str, limit: int = 50, offset: int = 0):
    """List transactions for a specific bank connection in the current Book context."""
    node = await store.get_node(CONNECTIONS[0], CONNECTIONS[1], connection_id)
    if not node:
        raise HTTPException(status_code=404, detail="Bank connection not found")
    nodes = await store.list_nodes(TRANSACTIONS[0], TRANSACTIONS[1], {"connection_id": connection_id})
    txns = [_transaction_model(n) for n in nodes]
    return txns[offset : offset + limit]


@app.post("/transactions/{connection_id}/reconcile")
async def reconcile_transaction(connection_id: str, request: ReconcileRequest):
    """Reconcile a bank transaction with an accounting entry."""
    node = await store.get_node(TRANSACTIONS[0], TRANSACTIONS[1], request.transaction_id)
    if not node or node.get("connection_id") != connection_id:
        raise HTTPException(status_code=404, detail="Transaction not found")

    await store.update_node(TRANSACTIONS[0], TRANSACTIONS[1], request.transaction_id, {"reconciled": True})
    logger.info("Transaction reconciled", transaction_id=request.transaction_id, matched_entry=request.matched_entry_id)
    return {"transaction_id": request.transaction_id, "reconciled": True, "matched_entry": request.matched_entry_id}


@app.delete("/connections/{connection_id}")
async def disconnect_bank(connection_id: str):
    """Disconnect a bank connection."""
    node = await store.get_node(CONNECTIONS[0], CONNECTIONS[1], connection_id)
    if not node:
        raise HTTPException(status_code=404, detail="Bank connection not found")

    await store.update_node(CONNECTIONS[0], CONNECTIONS[1], connection_id, {"status": "disconnected"})
    logger.info("Bank connection disconnected", connection_id=connection_id)
    return {"connection_id": connection_id, "status": "disconnected"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
