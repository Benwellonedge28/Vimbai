"""
Vimbai Bank Feed Integration Service
Connects to bank APIs to import transactions and reconcile with Vimbai records.

Durable records (connections, imported transactions, reconciliation rules,
sync history) persist in Neo4j, stamped with the caller and the Book context
(X-Book-ID verified upstream by the API gateway). Bank-side helpers (MT940
parsing, webhook signatures, provider fetch stubs) stay pure.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import hashlib
import hmac
import importlib.util
import json as _json
import os as _os
import sys as _sys
import uuid
from datetime import datetime, timedelta, timezone
from typing import Any, Dict, List, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "bank_feed_service" not in _sys.modules or not hasattr(_sys.modules.get("bank_feed_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("bank_feed_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["bank_feed_service"] = _pkg
    _sys.modules["bank_feed_service"].__path__ = [_HERE]

from bank_feed_service import crud
from bank_feed_service.database import Neo4jConnector
from bank_feed_service.dependencies import book_id_var, get_db_session, get_user_id
from bank_feed_service.exceptions import BankFeedError, NotFoundError
from bank_feed_service.models import (
    BANK_PROVIDERS,
    BankBalance,
    BankConnectionCreate,
    BankProvider,
    ReconciliationRule,
    SyncRequest,
    TransactionImport,
    TransactionStatus,
)
from dotenv import load_dotenv
from fastapi import BackgroundTasks, Depends, FastAPI, Request, status
from fastapi.responses import JSONResponse
from neo4j import AsyncSession

load_dotenv()

app = FastAPI(
    title="Vimbai Bank Feed Integration Service",
    description="Bank API integration, transaction importing, and automated reconciliation",
    version="2.0.0",
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="bank-feed-service", instrument_app=app)
except ImportError:
    TRACER = None
    import logging

    logging.getLogger(__name__).warning("OpenTelemetry not installed - tracing disabled")


@app.exception_handler(BankFeedError)
async def _bank_feed_error(request: Request, exc: BankFeedError):
    from fastapi.responses import JSONResponse

    resp = JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )
    if getattr(exc, "existing_id", None):
        resp.headers["X-Existing-Transaction-ID"] = exc.existing_id
    return resp


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


# ============================================================================
# Pure helpers (unchanged semantics)
# ============================================================================


def calculate_checksum(data: str) -> str:
    """Calculate checksum for data verification"""
    return hashlib.sha256(data.encode()).hexdigest()


def verify_webhook_signature(payload: bytes, signature: str, secret: str) -> bool:
    """Verify webhook signature from bank provider"""
    expected_signature = hmac.new(secret.encode(), payload, hashlib.sha256).hexdigest()
    return hmac.compare_digest(signature, expected_signature)


def parse_mt940_format(statement_data: str) -> List[Dict]:
    """Parse MT940 bank statement format"""
    transactions = []
    lines = statement_data.split("\n")

    current_tx = {}
    for line in lines:
        if line.startswith(":61:"):
            date_str = line[4:10]
            debit_credit = line[14]
            # amount runs to the next tag or end of line; MT940 uses comma decimals
            rest = line[15:]
            amount_str = rest[: rest.find(":")] if ":" in rest else rest
            amount = float(amount_str.replace(",", "."))
            current_tx = {
                "date": f"20{date_str[:2]}-{date_str[2:4]}-{date_str[4:6]}",
                "amount": float(amount) if debit_credit == "C" else -float(amount),
                "type": "credit" if debit_credit == "C" else "debit",
            }
        elif line.startswith(":82:"):
            current_tx["bank_ref"] = line[4:].strip()
        elif line.startswith(":86:"):
            current_tx["description"] = line[4:].strip()
            if current_tx:
                transactions.append(current_tx)
                current_tx = {}

    return transactions


async def fetch_plaid_transactions(access_token: str, start_date: str, end_date: str) -> List[Dict]:
    """Fetch transactions from Plaid API (provider integration stub)"""
    return []


async def fetch_stripe_balance(access_token: str) -> Dict:
    """Fetch balance from Stripe API (provider integration stub)"""
    return {"available": 0, "pending": 0, "currency": "usd"}


async def fetch_quickbooks_transactions(access_token: str, account_id: str) -> List[Dict]:
    """Fetch transactions from QuickBooks API (provider integration stub)"""
    return []


# ============================================================================
# Health
# ============================================================================


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "bank-feed-integration", "version": "2.0.0"}


# ============================================================================
# Bank Connection Management
# ============================================================================


@app.post("/connections", status_code=status.HTTP_201_CREATED)
async def create_bank_connection(
    connection: BankConnectionCreate,
    organization_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Register a new bank account connection"""
    return await crud.create_connection(db_session, user_id, connection, organization_id)


@app.get("/connections")
async def list_connections(
    organization_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's bank connections for an organization"""
    results = await crud.list_connections(db_session, user_id, organization_id)
    return {"total": len(results), "connections": results}


@app.get("/connections/{connection_id}")
async def get_connection(
    connection_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get bank connection details"""
    return await crud.get_connection(db_session, user_id, connection_id)


@app.delete("/connections/{connection_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_connection(
    connection_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Remove a bank connection"""
    await crud.delete_connection(db_session, user_id, connection_id)
    return {"ok": True}


# ============================================================================
# Transaction Import
# ============================================================================


@app.post("/transactions/import", status_code=status.HTTP_201_CREATED)
async def import_transaction(
    transaction: TransactionImport,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Import a single transaction from bank feed"""
    return await crud.import_transaction(db_session, user_id, transaction)


@app.post("/transactions/import-batch", status_code=status.HTTP_201_CREATED)
async def import_transactions_batch(
    transactions: List[TransactionImport],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Import multiple transactions"""
    results = []
    for tx_data in transactions:
        try:
            tx = await crud.import_transaction(db_session, user_id, tx_data)
            results.append({"status": "imported", "transaction_id": tx.id})
        except BankFeedError as e:
            if e.status_code == 409:  # Duplicate
                results.append(
                    {
                        "status": "duplicate",
                        "external_id": tx_data.external_id,
                        "existing_id": getattr(e, "existing_id", None),
                    }
                )
            else:
                results.append({"status": "failed", "error": str(e.detail if hasattr(e, "detail") else e)})

    imported = len([r for r in results if r["status"] == "imported"])
    duplicates = len([r for r in results if r["status"] == "duplicate"])

    return {
        "total": len(transactions),
        "imported": imported,
        "duplicates": duplicates,
        "failed": len(results) - imported - duplicates,
        "results": results,
    }


@app.post("/transactions/import-mt940")
async def import_mt940_statement(
    connection_id: str,
    statement_data: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Import transactions from MT940 bank statement format"""
    await crud.get_connection(db_session, user_id, connection_id)  # 404 if not caller's

    transactions = parse_mt940_format(statement_data)
    imported_txs = []
    for tx_data in transactions:
        tx = TransactionImport(
            bank_connection_id=connection_id,
            external_id=f"mt940_{tx_data.get('bank_ref', uuid.uuid4())}",
            date=datetime.fromisoformat(tx_data["date"]),
            amount=tx_data["amount"],
            description=tx_data.get("description", ""),
            transaction_type="credit" if tx_data["amount"] > 0 else "debit",
        )
        try:
            result = await crud.import_transaction(db_session, user_id, tx)
            imported_txs.append(result.id)
        except BankFeedError:
            continue  # Skip duplicates

    return {"parsed": len(transactions), "imported": len(imported_txs), "transaction_ids": imported_txs}


# ============================================================================
# Transaction Retrieval
# ============================================================================


@app.get("/transactions")
async def list_transactions(
    connection_id: Optional[str] = None,
    status: Optional[TransactionStatus] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100,
    offset: int = 0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's imported transactions with filters"""
    return await crud.list_transactions(
        db_session,
        user_id,
        connection_id=connection_id,
        status=status,
        start_date=start_date,
        end_date=end_date,
        limit=limit,
        offset=offset,
    )


@app.get("/transactions/{transaction_id}")
async def get_transaction(
    transaction_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get transaction details"""
    return await crud.get_transaction(db_session, user_id, transaction_id)


@app.put("/transactions/{transaction_id}/status")
async def update_transaction_status(
    transaction_id: str,
    status: TransactionStatus,
    notes: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update transaction status (reconcile, dispute, etc.)"""
    return await crud.update_transaction_status(db_session, user_id, transaction_id, status, notes)


@app.post("/transactions/{transaction_id}/link")
async def link_transaction(
    transaction_id: str,
    journal_entry_id: Optional[str] = None,
    invoice_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Link transaction to Vimbai entities"""
    return await crud.link_transaction(db_session, user_id, transaction_id, journal_entry_id, invoice_id)


# ============================================================================
# Bank Sync
# ============================================================================


@app.post("/sync/start")
async def start_sync(
    request: SyncRequest,
    background_tasks: BackgroundTasks,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Start bank sync operation"""
    conn = await crud.get_connection(db_session, user_id, request.bank_connection_id)

    sync_id = str(uuid.uuid4())
    # persist the sync record so /sync/{id} is queryable right away
    await crud.create_sync(db_session, user_id, sync_id, request.bank_connection_id)

    conn_prop_query = "x.last_sync_status = $status,\n        x.updated_at = datetime($updated_at)"
    from bank_feed_service import crud as _crud

    await _crud._set_node_props(
        db_session,
        user_id,
        "BankConnection",
        "OWNS_CONNECTION",
        conn.id,
        conn_prop_query,
        {"status": "syncing", "updated_at": datetime.now(timezone.utc).isoformat()},
    )

    background_tasks.add_task(
        perform_bank_sync,
        sync_id,
        user_id,
        request.bank_connection_id,
        request.start_date,
        request.end_date,
        request.force_full_sync,
    )

    return {"sync_id": sync_id, "status": "started"}


async def perform_bank_sync(
    sync_id: str,
    user_id: str,
    connection_id: str,
    start_date: Optional[datetime],
    end_date: Optional[datetime],
    force_full: bool,
):
    """Background task to perform bank sync (opens its own Neo4j session)."""
    from bank_feed_service.models import SyncStatus

    async with Neo4jConnector.get_driver().session() as session:
        if not end_date:
            end_date = datetime.now(timezone.utc)
        if not start_date:
            start_date = end_date - timedelta(days=30)

        try:
            conn = await crud.get_connection(session, user_id, connection_id)
            transactions = []
            if conn.provider == BankProvider.PLAID:
                transactions = await fetch_plaid_transactions(
                    conn.access_token_encrypted, start_date.isoformat(), end_date.isoformat()
                )
            elif conn.provider == BankProvider.QUICKBOOKS:
                transactions = await fetch_quickbooks_transactions(conn.access_token_encrypted, connection_id)

            imported_count = 0
            for tx_data in transactions:
                tx = TransactionImport(
                    bank_connection_id=connection_id,
                    external_id=tx_data.get("id", str(uuid.uuid4())),
                    date=datetime.fromisoformat(tx_data["date"]),
                    amount=float(tx_data["amount"]),
                    description=tx_data.get("description", ""),
                    merchant_name=tx_data.get("merchant_name"),
                    category=tx_data.get("category"),
                )
                try:
                    await crud.import_transaction(session, user_id, tx)
                    imported_count += 1
                except BankFeedError:
                    continue

            now = datetime.now(timezone.utc)
            await crud.update_sync(
                session, user_id, sync_id, SyncStatus.COMPLETED, imported=imported_count, completed=now
            )
            conn_prop_query = (
                "x.last_sync_at = datetime($last_sync_at),\n        "
                "x.last_sync_status = $last_sync_status,\n        "
                "x.error_message = $error_message,\n        "
                "x.updated_at = datetime($updated_at)"
            )
            await crud._set_node_props(
                session,
                user_id,
                "BankConnection",
                "OWNS_CONNECTION",
                connection_id,
                conn_prop_query,
                {
                    "last_sync_at": now.isoformat(),
                    "last_sync_status": SyncStatus.COMPLETED.value,
                    "error_message": None,
                    "updated_at": now.isoformat(),
                },
            )
        except Exception as e:  # noqa: BLE001 - original behavior recorded errors on the sync
            now = datetime.now(timezone.utc)
            try:
                current = await crud.get_sync(session, user_id, sync_id)
                errors = list(current.errors) + [str(e)]
                await crud.update_sync(session, user_id, sync_id, SyncStatus.FAILED, errors=errors, completed=now)
                await crud._set_node_props(
                    session,
                    user_id,
                    "BankConnection",
                    "OWNS_CONNECTION",
                    connection_id,
                    "x.last_sync_status = $last_sync_status,\n        x.error_message = $error_message,\n        x.updated_at = datetime($updated_at)",
                    {
                        "last_sync_status": SyncStatus.FAILED.value,
                        "error_message": str(e),
                        "updated_at": now.isoformat(),
                    },
                )
            except Exception:
                pass


@app.get("/sync/history")
async def get_sync_history(
    connection_id: Optional[str] = None,
    limit: int = 50,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's sync history"""
    return await crud.list_syncs(db_session, user_id, connection_id=connection_id, limit=limit)


@app.get("/sync/{sync_id}")
async def get_sync_status(
    sync_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get sync operation status"""
    return await crud.get_sync(db_session, user_id, sync_id)


# ============================================================================
# Balance Checking
# ============================================================================


@app.get("/connections/{connection_id}/balance")
async def get_account_balance(
    connection_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get current balance for a connected account (mock until provider integrations land)"""
    await crud.get_connection(db_session, user_id, connection_id)
    return BankBalance(
        account_id=connection_id,
        available_balance=10000.00,
        current_balance=10500.00,
        currency="USD",
        as_of_date=datetime.now(timezone.utc),
        pending_transactions=500.00,
    )


# ============================================================================
# Reconciliation Rules
# ============================================================================


@app.post("/rules", status_code=status.HTTP_201_CREATED)
async def create_reconciliation_rule(
    rule: ReconciliationRule,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a new reconciliation rule"""
    return await crud.create_rule(db_session, user_id, rule)


@app.get("/rules")
async def list_reconciliation_rules(
    active_only: bool = False,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's reconciliation rules"""
    results = await crud.list_rules(db_session, user_id, active_only=active_only)
    return {"total": len(results), "rules": results}


@app.put("/rules/{rule_id}")
async def update_reconciliation_rule(
    rule_id: str,
    update: ReconciliationRule,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update a reconciliation rule"""
    return await crud.update_rule(db_session, user_id, rule_id, update)


@app.delete("/rules/{rule_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_reconciliation_rule(
    rule_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Delete a reconciliation rule"""
    await crud.delete_rule(db_session, user_id, rule_id)
    return {"ok": True}


@app.post("/rules/apply")
async def apply_rules_to_transactions(
    transaction_ids: List[str],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Apply the caller's reconciliation rules to specific transactions"""
    results = []
    for tx_id in transaction_ids:
        try:
            tx = await crud.get_transaction(db_session, user_id, tx_id)
        except NotFoundError:
            continue
        matched = await crud.apply_rules_to_tx(db_session, user_id, tx)
        results.append({"transaction_id": tx_id, "matched": matched is not None, "rule_id": matched})

    return {"processed": len(results), "results": results}


# ============================================================================
# Webhook Handling
# ============================================================================


@app.post("/webhooks/{provider}")
async def handle_webhook(
    provider: str,
    request: Request,
    signature: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Handle webhook from bank provider (routed with the connection owner's context)"""
    from fastapi import HTTPException
    from fastapi import status as http_status

    if provider not in BANK_PROVIDERS:
        raise HTTPException(status_code=http_status.HTTP_400_BAD_REQUEST, detail="Unknown provider")

    if signature:
        # In production, verify using provider-specific secret
        pass

    try:
        data = _json.loads(await request.body())
    except _json.JSONDecodeError:
        raise HTTPException(status_code=http_status.HTTP_400_BAD_REQUEST, detail="Invalid JSON payload")

    if provider == "plaid":
        webhook_type = data.get("webhook_type")
        if webhook_type == "TRANSACTIONS":
            await handle_plaid_transaction_webhook(data, user_id, db_session)

    return {"status": "received"}


async def handle_plaid_transaction_webhook(data: Dict, user_id: str, db_session: AsyncSession):
    """Handle Plaid transaction webhook (imports only into caller-owned connections)"""
    transactions = data.get("transactions", [])
    for tx_data in transactions:
        tx = TransactionImport(
            bank_connection_id=data.get("connection_id", ""),
            external_id=tx_data.get("transaction_id", str(uuid.uuid4())),
            date=datetime.fromisoformat(tx_data.get("date")),
            amount=float(tx_data.get("amount", 0)),
            description=tx_data.get("name", ""),
            merchant_name=tx_data.get("merchant_name"),
            category=tx_data.get("category"),
        )
        try:
            await crud.import_transaction(db_session, user_id, tx)
        except BankFeedError:
            continue


# ============================================================================
# Statistics
# ============================================================================


@app.get("/statistics")
async def get_statistics(
    organization_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get bank integration statistics for the caller's records"""
    return await crud.statistics(db_session, user_id, organization_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8067)
