"""
FinAcc Bank Feed Integration Service
Connects to bank APIs to import transactions and reconcile with FinAcc records
"""

from fastapi import FastAPI, HTTPException, BackgroundTasks, Depends, status
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field
from typing import List, Optional, Dict, Any
from datetime import datetime, timezone, timedelta
from enum import Enum
import asyncio
import json
import uuid
import hashlib
import hmac
from dotenv import load_dotenv

load_dotenv()

app = FastAPI(
    title="FinAcc Bank Feed Integration Service",
    description="Bank API integration, transaction importing, and automated reconciliation",
    version="1.0.0",
)

# ============================================================================
# Configuration
# ============================================================================

BANK_PROVIDERS = {
    "plaid": {
        "name": "Plaid",
        "base_url": "https://production.plaid.com",
        "supports_balance": True,
        "supports_transactions": True,
        "supports_transfer": False
    },
    "stripe": {
        "name": "Stripe",
        "base_url": "https://api.stripe.com",
        "supports_balance": True,
        "supports_transactions": False,
        "supports_transfer": True
    },
    "quickbooks": {
        "name": "QuickBooks",
        "base_url": "https://quickbooks.api.intuit.com",
        "supports_balance": True,
        "supports_transactions": True,
        "supports_transfer": True
    },
    "xero": {
        "name": "Xero",
        "base_url": "https://api.xero.com",
        "supports_balance": True,
        "supports_transactions": True,
        "supports_transfer": True
    },
    "manual": {
        "name": "Manual Import",
        "base_url": None,
        "supports_balance": True,
        "supports_transactions": True,
        "supports_transfer": False
    }
}

# ============================================================================
# Models
# ============================================================================

class BankProvider(str, Enum):
    PLAID = "plaid"
    STRIPE = "stripe"
    QUICKBOOKS = "quickbooks"
    XERO = "xero"
    MANUAL = "manual"

class AccountType(str, Enum):
    CHECKING = "checking"
    SAVINGS = "savings"
    CREDIT_CARD = "credit_card"
    INVESTMENT = "investment"
    LOAN = "loan"
    MONEY_MARKET = "money_market"

class TransactionStatus(str, Enum):
    PENDING = "pending"
    CLEARED = "cleared"
    RECONCILED = "reconciled"
    DISPUTED = "disputed"
    RETURNED = "returned"

class SyncStatus(str, Enum):
    PENDING = "pending"
    SYNCING = "syncing"
    COMPLETED = "completed"
    FAILED = "failed"

class BankConnectionCreate(BaseModel):
    provider: BankProvider
    account_name: str
    account_type: AccountType
    account_number_last4: str = Field(..., max_length=4)
    routing_number: Optional[str] = None
    access_token_encrypted: Optional[str] = None  # Encrypted in production
    webhook_url: Optional[str] = None
    auto_sync_enabled: bool = True
    sync_interval_minutes: int = 60

class BankConnection(BankConnectionCreate):
    id: str
    organization_id: str
    status: str = "active"
    last_sync_at: Optional[datetime] = None
    last_sync_status: Optional[SyncStatus] = None
    error_message: Optional[str] = None
    created_at: datetime
    updated_at: datetime

class TransactionImport(BaseModel):
    bank_connection_id: str
    external_id: str
    date: datetime
    amount: float
    currency: str = "USD"
    description: str
    category: Optional[str] = None
    merchant_name: Optional[str] = None
    merchant_id: Optional[str] = None
    transaction_type: str = "debit"  # debit or credit
    pending: bool = False
    metadata: Optional[Dict[str, Any]] = None

class TransactionInDB(TransactionImport):
    id: str
    bank_connection_id: str
    linked_journal_entry_id: Optional[str] = None
    linked_invoice_id: Optional[str] = None
    matched_rule_id: Optional[str] = None
    status: TransactionStatus
    confidence_score: float = 0.0
    imported_at: datetime
    created_at: datetime
    updated_at: datetime

class ReconciliationRule(BaseModel):
    id: str
    name: str
    description: Optional[str] = None
    match_conditions: Dict[str, Any]  # conditions for auto-matching
    priority: int = 0
    auto_match_enabled: bool = True
    create_journal_entry: bool = False
    journal_entry_template: Optional[Dict[str, Any]] = None
    active: bool = True

class BankBalance(BaseModel):
    account_id: str
    available_balance: float
    current_balance: float
    currency: str = "USD"
    as_of_date: datetime
    pending_transactions: float = 0.0

class SyncRequest(BaseModel):
    bank_connection_id: str
    start_date: Optional[datetime] = None
    end_date: Optional[datetime] = None
    force_full_sync: bool = False

class SyncResult(BaseModel):
    sync_id: str
    bank_connection_id: str
    status: SyncStatus
    transactions_imported: int = 0
    transactions_updated: int = 0
    transactions_matched: int = 0
    errors: List[str] = []
    started_at: datetime
    completed_at: Optional[datetime] = None

# ============================================================================
# In-Memory Storage
# ============================================================================

bank_connections: Dict[str, BankConnection] = {}
imported_transactions: Dict[str, TransactionInDB] = {}
reconciliation_rules: Dict[str, ReconciliationRule] = {}
sync_history: List[SyncResult] = []

# ============================================================================
# Helper Functions
# ============================================================================

def calculate_checksum(data: str) -> str:
    """Calculate checksum for data verification"""
    return hashlib.sha256(data.encode()).hexdigest()

def verify_webhook_signature(payload: bytes, signature: str, secret: str) -> bool:
    """Verify webhook signature from bank provider"""
    expected_signature = hmac.new(
        secret.encode(),
        payload,
        hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(signature, expected_signature)

def parse_mt940_format(statement_data: str) -> List[Dict]:
    """Parse MT940 bank statement format"""
    transactions = []
    lines = statement_data.split('\n')

    current_tx = {}
    for line in lines:
        if line.startswith(':61:'):  # Transaction line
            # Parse :61:DDMMYYMMDDC...CR...
            date_str = line[4:10]
            entry_date = line[10:14]
            debit_credit = line[14]
            amount = line[15:line.find(':')]

            current_tx = {
                'date': f"20{date_str[:2]}-{date_str[2:4]}-{date_str[4:6]}",
                'amount': float(amount) if debit_credit == 'C' else -float(amount),
                'type': 'credit' if debit_credit == 'C' else 'debit'
            }
        elif line.startswith(':82:'):  # Bank reference
            current_tx['bank_ref'] = line[4:].strip()
        elif line.startswith(':86:'):  # Transaction description
            current_tx['description'] = line[4:].strip()
            if current_tx:
                transactions.append(current_tx)
                current_tx = {}

    return transactions

async def fetch_plaid_transactions(access_token: str, start_date: str, end_date: str) -> List[Dict]:
    """Fetch transactions from Plaid API"""
    # In production, use plaid-python library
    # This is a placeholder for the integration
    return []

async def fetch_stripe_balance(access_token: str) -> Dict:
    """Fetch balance from Stripe API"""
    # In production, use stripe-python library
    return {"available": 0, "pending": 0, "currency": "usd"}

async def fetch_quickbooks_transactions(access_token: str, account_id: str) -> List[Dict]:
    """Fetch transactions from QuickBooks API"""
    # In production, use intuit-oauth library
    return []

async def apply_reconciliation_rules(transaction: TransactionInDB) -> Optional[str]:
    """Apply reconciliation rules to auto-match transaction"""
    matched_rule_id = None

    for rule in sorted(reconciliation_rules.values(), key=lambda r: r.priority, reverse=True):
        if not rule.active or not rule.auto_match_enabled:
            continue

        conditions = rule.match_conditions
        match = True

        # Check amount range
        if 'amount_min' in conditions or 'amount_max' in conditions:
            amount = abs(transaction.amount)
            if 'amount_min' in conditions and amount < conditions['amount_min']:
                match = False
            if 'amount_max' in conditions and amount > conditions['amount_max']:
                match = False

        # Check merchant pattern
        if 'merchant_pattern' in conditions and transaction.merchant_name:
            pattern = conditions['merchant_pattern'].lower()
            if pattern not in transaction.merchant_name.lower():
                match = False

        # Check category
        if 'categories' in conditions and transaction.category:
            if transaction.category not in conditions['categories']:
                match = False

        # Check transaction type
        if 'transaction_type' in conditions:
            if transaction.transaction_type != conditions['transaction_type']:
                match = False

        if match:
            matched_rule_id = rule.id
            transaction.matched_rule_id = rule.id
            transaction.confidence_score = 0.85
            break

    return matched_rule_id

# ============================================================================
# API Endpoints
# ============================================================================

@app.get("/")
async def health_check():
    return {
        "status": "healthy",
        "service": "bank-feed-integration",
        "total_connections": len(bank_connections),
        "total_transactions": len(imported_transactions)
    }

# --- Bank Connection Management ---

@app.post("/connections", status_code=status.HTTP_201_CREATED)
async def create_bank_connection(
    connection: BankConnectionCreate,
    organization_id: str
):
    """Register a new bank account connection"""
    conn_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)

    bank_conn = BankConnection(
        id=conn_id,
        organization_id=organization_id,
        provider=connection.provider,
        account_name=connection.account_name,
        account_type=connection.account_type,
        account_number_last4=connection.account_number_last4,
        routing_number=connection.routing_number,
        access_token_encrypted=connection.access_token_encrypted,
        webhook_url=connection.webhook_url,
        auto_sync_enabled=connection.auto_sync_enabled,
        sync_interval_minutes=connection.sync_interval_minutes,
        created_at=now,
        updated_at=now
    )

    bank_connections[conn_id] = bank_conn
    return bank_conn

@app.get("/connections")
async def list_connections(organization_id: str):
    """List all bank connections for an organization"""
    results = [c for c in bank_connections.values() if c.organization_id == organization_id]
    return {"total": len(results), "connections": results}

@app.get("/connections/{connection_id}")
async def get_connection(connection_id: str):
    """Get bank connection details"""
    if connection_id not in bank_connections:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Connection not found")
    return bank_connections[connection_id]

@app.delete("/connections/{connection_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_connection(connection_id: str):
    """Remove a bank connection"""
    if connection_id not in bank_connections:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Connection not found")

    del bank_connections[connection_id]
    return {"ok": True}

# --- Transaction Import ---

@app.post("/transactions/import", status_code=status.HTTP_201_CREATED)
async def import_transaction(transaction: TransactionImport):
    """Import a single transaction from bank feed"""
    conn_id = transaction.bank_connection_id
    if conn_id not in bank_connections:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Bank connection not found")

    # Check for duplicate
    for existing in imported_transactions.values():
        if existing.external_id == transaction.external_id and existing.bank_connection_id == conn_id:
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Transaction already imported",
                headers={"X-Existing-Transaction-ID": existing.id}
            )

    tx_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)

    tx = TransactionInDB(
        id=tx_id,
        bank_connection_id=conn_id,
        external_id=transaction.external_id,
        date=transaction.date,
        amount=transaction.amount,
        currency=transaction.currency,
        description=transaction.description,
        category=transaction.category,
        merchant_name=transaction.merchant_name,
        merchant_id=transaction.merchant_id,
        transaction_type=transaction.transaction_type,
        pending=transaction.pending,
        metadata=transaction.metadata,
        status=TransactionStatus.PENDING if transaction.pending else TransactionStatus.CLEARED,
        confidence_score=0.0,
        imported_at=now,
        created_at=now,
        updated_at=now
    )

    # Apply reconciliation rules
    await apply_reconciliation_rules(tx)

    imported_transactions[tx_id] = tx
    return tx

@app.post("/transactions/import-batch", status_code=status.HTTP_201_CREATED)
async def import_transactions_batch(transactions: List[TransactionImport]):
    """Import multiple transactions"""
    results = []

    for tx_data in transactions:
        try:
            tx = await import_transaction(tx_data)
            results.append({"status": "imported", "transaction_id": tx.id})
        except HTTPException as e:
            if e.status_code == 409:  # Duplicate
                results.append({
                    "status": "duplicate",
                    "external_id": tx_data.external_id,
                    "existing_id": e.headers.get("X-Existing-Transaction-ID")
                })
            else:
                results.append({"status": "failed", "error": e.detail})

    imported = len([r for r in results if r["status"] == "imported"])
    duplicates = len([r for r in results if r["status"] == "duplicate"])

    return {
        "total": len(transactions),
        "imported": imported,
        "duplicates": duplicates,
        "failed": len(results) - imported - duplicates,
        "results": results
    }

@app.post("/transactions/import-mt940")
async def import_mt940_statement(connection_id: str, statement_data: str):
    """Import transactions from MT940 bank statement format"""
    if connection_id not in bank_connections:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Connection not found")

    transactions = parse_mt940_format(statement_data)

    imported_txs = []
    for tx_data in transactions:
        tx = TransactionImport(
            bank_connection_id=connection_id,
            external_id=f"mt940_{tx_data.get('bank_ref', uuid.uuid4())}",
            date=datetime.fromisoformat(tx_data['date']),
            amount=tx_data['amount'],
            description=tx_data.get('description', ''),
            transaction_type='credit' if tx_data['amount'] > 0 else 'debit'
        )

        try:
            result = await import_transaction(tx)
            imported_txs.append(result.id)
        except HTTPException:
            continue  # Skip duplicates

    return {
        "parsed": len(transactions),
        "imported": len(imported_txs),
        "transaction_ids": imported_txs
    }

# --- Transaction Retrieval ---

@app.get("/transactions")
async def list_transactions(
    connection_id: Optional[str] = None,
    status: Optional[TransactionStatus] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100,
    offset: int = 0
):
    """List imported transactions with filters"""
    results = list(imported_transactions.values())

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
    results = results[offset:offset + limit]

    return {"total": total, "transactions": results}

@app.get("/transactions/{transaction_id}")
async def get_transaction(transaction_id: str):
    """Get transaction details"""
    if transaction_id not in imported_transactions:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Transaction not found")
    return imported_transactions[transaction_id]

@app.put("/transactions/{transaction_id}/status")
async def update_transaction_status(
    transaction_id: str,
    status: TransactionStatus,
    notes: Optional[str] = None
):
    """Update transaction status (reconcile, dispute, etc.)"""
    if transaction_id not in imported_transactions:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Transaction not found")

    tx = imported_transactions[transaction_id]
    tx.status = status
    tx.updated_at = datetime.now(timezone.utc)

    if notes:
        tx.metadata = tx.metadata or {}
        tx.metadata["status_notes"] = notes

    return tx

@app.post("/transactions/{transaction_id}/link")
async def link_transaction(
    transaction_id: str,
    journal_entry_id: Optional[str] = None,
    invoice_id: Optional[str] = None
):
    """Link transaction to FinAcc entities"""
    if transaction_id not in imported_transactions:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Transaction not found")

    tx = imported_transactions[transaction_id]
    if journal_entry_id:
        tx.linked_journal_entry_id = journal_entry_id
    if invoice_id:
        tx.linked_invoice_id = invoice_id

    tx.status = TransactionStatus.RECONCILED
    tx.updated_at = datetime.now(timezone.utc)

    return tx

# --- Bank Sync ---

@app.post("/sync/start")
async def start_sync(request: SyncRequest, background_tasks: BackgroundTasks):
    """Start bank sync operation"""
    if request.bank_connection_id not in bank_connections:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Connection not found")

    conn = bank_connections[request.bank_connection_id]
    conn.last_sync_status = SyncStatus.SYNCING
    conn.updated_at = datetime.now(timezone.utc)

    sync_id = str(uuid.uuid4())
    sync_result = SyncResult(
        sync_id=sync_id,
        bank_connection_id=request.bank_connection_id,
        status=SyncStatus.SYNCING,
        started_at=datetime.now(timezone.utc)
    )

    # Process in background
    background_tasks.add_task(
        perform_bank_sync,
        sync_id,
        request.bank_connection_id,
        request.start_date,
        request.end_date,
        request.force_full_sync
    )

    return {"sync_id": sync_id, "status": "started"}

async def perform_bank_sync(
    sync_id: str,
    connection_id: str,
    start_date: Optional[datetime],
    end_date: Optional[datetime],
    force_full: bool
):
    """Background task to perform bank sync"""
    conn = bank_connections[connection_id]

    # Default date range
    if not end_date:
        end_date = datetime.now(timezone.utc)
    if not start_date:
        start_date = end_date - timedelta(days=30)

    sync_result = None

    # Find existing sync result
    for sr in sync_history:
        if sr.sync_id == sync_id:
            sync_result = sr
            break

    if not sync_result:
        sync_result = SyncResult(
            sync_id=sync_id,
            bank_connection_id=connection_id,
            status=SyncStatus.SYNCING,
            started_at=datetime.now(timezone.utc)
        )
        sync_history.append(sync_result)

    try:
        # Fetch based on provider
        if conn.provider == BankProvider.PLAID:
            transactions = await fetch_plaid_transactions(
                conn.access_token_encrypted,
                start_date.isoformat(),
                end_date.isoformat()
            )
        elif conn.provider == BankProvider.STRIPE:
            balance = await fetch_stripe_balance(conn.access_token_encrypted)
        elif conn.provider == BankProvider.QUICKBOOKS:
            transactions = await fetch_quickbooks_transactions(
                conn.access_token_encrypted,
                connection_id
            )
        else:
            # Manual or other providers
            transactions = []

        # Import transactions
        imported_count = 0
        for tx_data in transactions:
            tx = TransactionImport(
                bank_connection_id=connection_id,
                external_id=tx_data.get('id', str(uuid.uuid4())),
                date=datetime.fromisoformat(tx_data['date']),
                amount=float(tx_data['amount']),
                description=tx_data.get('description', ''),
                merchant_name=tx_data.get('merchant_name'),
                category=tx_data.get('category')
            )

            try:
                await import_transaction(tx)
                imported_count += 1
            except HTTPException:
                continue

        sync_result.transactions_imported = imported_count
        sync_result.status = SyncStatus.COMPLETED
        sync_result.completed_at = datetime.now(timezone.utc)

        conn.last_sync_at = datetime.now(timezone.utc)
        conn.last_sync_status = SyncStatus.COMPLETED
        conn.error_message = None

    except Exception as e:
        sync_result.status = SyncStatus.FAILED
        sync_result.errors.append(str(e))
        sync_result.completed_at = datetime.now(timezone.utc)

        conn.last_sync_status = SyncStatus.FAILED
        conn.error_message = str(e)

    conn.updated_at = datetime.now(timezone.utc)

@app.get("/sync/{sync_id}")
async def get_sync_status(sync_id: str):
    """Get sync operation status"""
    for sr in sync_history:
        if sr.sync_id == sync_id:
            return sr
    raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Sync not found")

@app.get("/sync/history")
async def get_sync_history(connection_id: Optional[str] = None, limit: int = 50):
    """Get sync history"""
    results = sync_history

    if connection_id:
        results = [s for s in results if s.bank_connection_id == connection_id]

    results.sort(key=lambda x: x.started_at, reverse=True)
    return {"total": len(results), "history": results[:limit]}

# --- Balance Checking ---

@app.get("/connections/{connection_id}/balance")
async def get_account_balance(connection_id: str):
    """Get current balance for connected account"""
    if connection_id not in bank_connections:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Connection not found")

    conn = bank_connections[connection_id]

    # In production, fetch from actual bank API
    # For now, return mock balance
    balance = BankBalance(
        account_id=connection_id,
        available_balance=10000.00,
        current_balance=10500.00,
        currency="USD",
        as_of_date=datetime.now(timezone.utc),
        pending_transactions=500.00
    )

    return balance

# --- Reconciliation Rules ---

@app.post("/rules", status_code=status.HTTP_201_CREATED)
async def create_reconciliation_rule(rule: ReconciliationRule):
    """Create a new reconciliation rule"""
    rule_id = str(uuid.uuid4())
    rule_dict = rule.model_dump()
    rule_dict["id"] = rule_id

    reconciliation_rules[rule_id] = ReconciliationRule(**rule_dict)
    return reconciliation_rules[rule_id]

@app.get("/rules")
async def list_reconciliation_rules(active_only: bool = False):
    """List all reconciliation rules"""
    results = list(reconciliation_rules.values())

    if active_only:
        results = [r for r in results if r.active]

    return {"total": len(results), "rules": results}

@app.put("/rules/{rule_id}")
async def update_reconciliation_rule(rule_id: str, update: ReconciliationRule):
    """Update a reconciliation rule"""
    if rule_id not in reconciliation_rules:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Rule not found")

    rule_dict = update.model_dump()
    rule_dict["id"] = rule_id
    reconciliation_rules[rule_id] = ReconciliationRule(**rule_dict)

    return reconciliation_rules[rule_id]

@app.delete("/rules/{rule_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_reconciliation_rule(rule_id: str):
    """Delete a reconciliation rule"""
    if rule_id not in reconciliation_rules:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Rule not found")

    del reconciliation_rules[rule_id]
    return {"ok": True}

@app.post("/rules/apply")
async def apply_rules_to_transactions(transaction_ids: List[str]):
    """Apply reconciliation rules to specific transactions"""
    results = []

    for tx_id in transaction_ids:
        if tx_id in imported_transactions:
            matched = await apply_reconciliation_rules(imported_transactions[tx_id])
            results.append({
                "transaction_id": tx_id,
                "matched": matched is not None,
                "rule_id": matched
            })

    return {"processed": len(results), "results": results}

# --- Webhook Handling ---

@app.post("/webhooks/{provider}")
async def handle_webhook(
    provider: str,
    payload: bytes,
    signature: Optional[str] = None
):
    """Handle webhook from bank provider"""
    if provider not in BANK_PROVIDERS:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Unknown provider")

    # Verify signature if provided
    if signature:
        # In production, verify using provider-specific secret
        pass

    # Parse webhook payload
    try:
        data = json.loads(payload)
    except json.JSONDecodeError:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Invalid JSON payload")

    # Handle based on provider
    if provider == "plaid":
        webhook_type = data.get("webhook_type")
        if webhook_type == "TRANSACTIONS":
            await handle_plaid_transaction_webhook(data)

    return {"status": "received"}

async def handle_plaid_transaction_webhook(data: Dict):
    """Handle Plaid transaction webhook"""
    # Process new transactions from Plaid
    transactions = data.get("transactions", [])

    for tx_data in transactions:
        tx = TransactionImport(
            bank_connection_id=data.get("connection_id", ""),
            external_id=tx_data.get("transaction_id", str(uuid.uuid4())),
            date=datetime.fromisoformat(tx_data.get("date")),
            amount=float(tx_data.get("amount", 0)),
            description=tx_data.get("name", ""),
            merchant_name=tx_data.get("merchant_name"),
            category=tx_data.get("category")
        )

        try:
            await import_transaction(tx)
        except HTTPException:
            continue

# --- Statistics ---

@app.get("/statistics")
async def get_statistics(organization_id: str):
    """Get bank integration statistics"""
    org_connections = [c for c in bank_connections.values() if c.organization_id == organization_id]

    total_transactions = 0
    reconciled = 0
    pending = 0

    for conn in org_connections:
        conn_txs = [t for t in imported_transactions.values() if t.bank_connection_id == conn.id]
        total_transactions += len(conn_txs)
        reconciled += len([t for t in conn_txs if t.status == TransactionStatus.RECONCILED])
        pending += len([t for t in conn_txs if t.status == TransactionStatus.PENDING])

    by_provider = {}
    for conn in org_connections:
        provider = conn.provider.value
        if provider not in by_provider:
            by_provider[provider] = {"connections": 0, "transactions": 0}
        by_provider[provider]["connections"] += 1
        by_provider[provider]["transactions"] += len([
            t for t in imported_transactions.values() if t.bank_connection_id == conn.id
        ])

    return {
        "total_connections": len(org_connections),
        "total_transactions": total_transactions,
        "reconciled": reconciled,
        "pending": pending,
        "reconciliation_rate": round(reconciled / total_transactions * 100, 2) if total_transactions else 0,
        "by_provider": by_provider
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8097)