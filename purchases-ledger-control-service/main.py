"""
FinAcc Purchases Ledger Control Service
Manages purchases ledger control account and creditor transactions.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "purchases-ledger-control-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8039"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Purchases Ledger Control Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class TransactionType(str):
    PURCHASE_INVOICE = "purchase_invoice"
    DEBIT_NOTE = "debit_note"
    PAYMENT = "payment"
    REFUND = "refund"
    DISCOUNT = "discount"


class CreditorTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    transaction_type: TransactionType
    creditor_id: str
    creditor_name: str
    invoice_number: Optional[str] = None
    date: datetime
    amount: float
    balance: float = 0
    reference: Optional[str] = None
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ControlAccountSummary(BaseModel):
    as_of_date: datetime
    total_invoices: float = 0
    total_debit_notes: float = 0
    total_payments: float = 0
    closing_balance: float = 0
    transaction_count: int = 0


# In-memory storage
creditor_transactions: List[CreditorTransaction] = []
creditor_balances: Dict[str, float] = {}


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "POST":
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception:
        return {}


async def call_audit_service(action: str, resource_type: str, resource_id: str, details: Dict[str, Any]):
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            await client.post(f"{AUDIT_SERVICE_URL}/audit", json={
                "action": action, "resource_type": resource_type, "resource_id": resource_id,
                "details": details, "timestamp": datetime.utcnow().isoformat()
            })
    except Exception:
        pass


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Purchases ledger control account management"}


@app.post("/transactions")
async def record_transaction(
    transaction_type: TransactionType, creditor_id: str, creditor_name: str,
    date: datetime, amount: float, invoice_number: Optional[str] = None, reference: Optional[str] = None
):
    """Record a creditor transaction."""
    txn = CreditorTransaction(
        transaction_type=transaction_type, creditor_id=creditor_id, creditor_name=creditor_name,
        invoice_number=invoice_number, date=date, amount=amount, reference=reference
    )

    current_balance = creditor_balances.get(creditor_id, 0)
    if transaction_type in [TransactionType.PURCHASE_INVOICE]:
        txn.balance = current_balance + amount
        creditor_balances[creditor_id] = txn.balance
    elif transaction_type in [TransactionType.DEBIT_NOTE, TransactionType.PAYMENT, TransactionType.REFUND]:
        txn.balance = current_balance - amount
        creditor_balances[creditor_id] = txn.balance

    creditor_transactions.append(txn)

    # Create journal entry
    entries = []
    if transaction_type == TransactionType.PURCHASE_INVOICE:
        entries = [
            {"account_code": "5000", "description": "Purchases", "debit": amount, "credit": 0},
            {"account_code": "2100", "description": "Accounts Payable", "debit": 0, "credit": amount},
        ]
    elif transaction_type == TransactionType.PAYMENT:
        entries = [
            {"account_code": "2100", "description": "Accounts Payable", "debit": amount, "credit": 0},
            {"account_code": "1000", "description": "Cash/Bank", "debit": 0, "credit": amount},
        ]

    if entries:
        journal_entry = {"date": date, "description": f"{transaction_type} - {creditor_name}", "entries": entries}
        result = await call_accounting_service("POST", "/journal-entries", journal_entry)
        txn.journal_entry_id = result.get("id")

    await call_audit_service("CREATE", "transaction", txn.id, {"type": transaction_type, "amount": amount})
    return txn


@app.get("/control-account/summary")
async def get_control_summary(as_of_date: Optional[datetime] = None):
    """Get control account summary."""
    as_of_date = as_of_date or datetime.utcnow()
    transactions = [t for t in creditor_transactions if t.date <= as_of_date]

    total_invoices = sum(t.amount for t in transactions if t.transaction_type == TransactionType.PURCHASE_INVOICE)
    total_debit_notes = sum(t.amount for t in transactions if t.transaction_type == TransactionType.DEBIT_NOTE)
    total_payments = sum(t.amount for t in transactions if t.transaction_type == TransactionType.PAYMENT)

    closing_balance = total_invoices - total_debit_notes - total_payments

    return ControlAccountSummary(
        as_of_date=as_of_date, total_invoices=total_invoices,
        total_debit_notes=total_debit_notes, total_payments=total_payments,
        closing_balance=closing_balance, transaction_count=len(transactions)
    )


@app.get("/creditors")
async def list_creditors():
    """List all creditors with balances."""
    return {"creditors": [{"creditor_id": cid, "balance": bal} for cid, bal in creditor_balances.items()],
            "total_balance": sum(creditor_balances.values())}


@app.post("/reconcile")
async def reconcile_control_account():
    """Reconcile control account."""
    control_balance = sum(creditor_balances.values())
    summary = await get_control_summary()
    return {
        "control_account_balance": summary.closing_balance,
        "purchases_ledger_total": control_balance,
        "difference": abs(summary.closing_balance - control_balance),
        "reconciled": abs(summary.closing_balance - control_balance) < 0.01
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)