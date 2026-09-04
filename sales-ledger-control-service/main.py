"""
Vimbai Sales Ledger Control Service
Manages sales ledger control account and debtor transactions.
"""

import os
import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "sales-ledger-control-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8038"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

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

app = FastAPI(title="Vimbai Sales Ledger Control Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class TransactionType(str, Enum):
    INVOICE = "invoice"
    CREDIT_NOTE = "credit_note"
    PAYMENT = "payment"
    REFUND = "refund"
    BAD_DEBT = "bad_debt"
    DISCOUNT = "discount"


class DebtorTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    transaction_type: TransactionType
    debtor_id: str
    debtor_name: str
    invoice_number: Optional[str] = None
    date: datetime
    amount: float
    balance: float = 0
    reference: Optional[str] = None
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ControlAccountSummary(BaseModel):
    as_of_date: datetime
    opening_balance: float = 0
    total_invoices: float = 0
    total_credit_notes: float = 0
    total_payments: float = 0
    total_bad_debts: float = 0
    closing_balance: float = 0
    transaction_count: int = 0
    debtor_count: int = 0


# In-memory storage
debtor_transactions: List[DebtorTransaction] = []
debtor_balances: Dict[str, float] = {}


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
            await client.post(
                f"{AUDIT_SERVICE_URL}/audit",
                json={
                    "action": action,
                    "resource_type": resource_type,
                    "resource_id": resource_id,
                    "details": details,
                    "timestamp": datetime.utcnow().isoformat(),
                },
            )
    except Exception:
        pass


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "description": "Sales ledger control account management",
    }


@app.post("/transactions")
async def record_transaction(
    transaction_type: TransactionType,
    debtor_id: str,
    debtor_name: str,
    date: datetime,
    amount: float,
    invoice_number: Optional[str] = None,
    reference: Optional[str] = None,
):
    """Record a debtor transaction."""
    txn = DebtorTransaction(
        transaction_type=transaction_type,
        debtor_id=debtor_id,
        debtor_name=debtor_name,
        invoice_number=invoice_number,
        date=date,
        amount=amount,
        reference=reference,
    )

    # Calculate balance
    current_balance = debtor_balances.get(debtor_id, 0)
    if transaction_type in [TransactionType.INVOICE]:
        txn.balance = current_balance + amount
        debtor_balances[debtor_id] = txn.balance
    elif transaction_type in [
        TransactionType.CREDIT_NOTE,
        TransactionType.PAYMENT,
        TransactionType.REFUND,
        TransactionType.BAD_DEBT,
    ]:
        txn.balance = current_balance - amount
        debtor_balances[debtor_id] = txn.balance

    debtor_transactions.append(txn)

    # Create journal entry
    entries = []
    if transaction_type == TransactionType.INVOICE:
        entries = [
            {"account_code": "1100", "description": "Accounts Receivable", "debit": amount, "credit": 0},
            {"account_code": "4000", "description": "Sales Revenue", "debit": 0, "credit": amount},
        ]
    elif transaction_type == TransactionType.CREDIT_NOTE:
        entries = [
            {"account_code": "4000", "description": "Sales Returns", "debit": amount, "credit": 0},
            {"account_code": "1100", "description": "Accounts Receivable", "debit": 0, "credit": amount},
        ]
    elif transaction_type == TransactionType.PAYMENT:
        entries = [
            {"account_code": "1000", "description": "Cash/Bank", "debit": amount, "credit": 0},
            {"account_code": "1100", "description": "Accounts Receivable", "debit": 0, "credit": amount},
        ]

    if entries:
        journal_entry = {
            "date": date,
            "description": f"{transaction_type} - {debtor_name}",
            "entries": entries,
            "reference": reference or f"SL-{txn.id[:8]}",
        }
        result = await call_accounting_service("POST", "/journal-entries", journal_entry)
        txn.journal_entry_id = result.get("id")

    await call_audit_service("CREATE", "transaction", txn.id, {"type": transaction_type, "amount": amount})
    return txn


@app.get("/control-account/summary")
async def get_control_summary(as_of_date: Optional[datetime] = None):
    """Get control account summary."""
    as_of_date = as_of_date or datetime.utcnow()

    transactions = [t for t in debtor_transactions if t.date <= as_of_date]

    total_invoices = sum(t.amount for t in transactions if t.transaction_type == TransactionType.INVOICE)
    total_credit_notes = sum(t.amount for t in transactions if t.transaction_type == TransactionType.CREDIT_NOTE)
    total_payments = sum(t.amount for t in transactions if t.transaction_type == TransactionType.PAYMENT)
    total_bad_debts = sum(t.amount for t in transactions if t.transaction_type == TransactionType.BAD_DEBT)

    closing_balance = total_invoices - total_credit_notes - total_payments - total_bad_debts

    return ControlAccountSummary(
        as_of_date=as_of_date,
        total_invoices=total_invoices,
        total_credit_notes=total_credit_notes,
        total_payments=total_payments,
        total_bad_debts=total_bad_debts,
        closing_balance=closing_balance,
        transaction_count=len(transactions),
        debtor_count=len(set(t.debtor_id for t in transactions)),
    )


@app.get("/debtors/{debtor_id}/balance")
async def get_debtor_balance(debtor_id: str):
    """Get individual debtor balance."""
    transactions = [t for t in debtor_transactions if t.debtor_id == debtor_id]
    balance = debtor_balances.get(debtor_id, 0)

    return {
        "debtor_id": debtor_id,
        "balance": balance,
        "transaction_count": len(transactions),
        "transactions": transactions[-10:],
    }


@app.get("/debtors")
async def list_debtors():
    """List all debtors with balances."""
    return {
        "debtors": [{"debtor_id": did, "balance": bal} for did, bal in debtor_balances.items()],
        "total_balance": sum(debtor_balances.values()),
    }


@app.post("/reconcile")
async def reconcile_control_account():
    """Reconcile control account."""
    control_balance = sum(debtor_balances.values())
    summary = await get_control_summary()

    return {
        "control_account_balance": summary.closing_balance,
        "sales_ledger_total": control_balance,
        "difference": abs(summary.closing_balance - control_balance),
        "reconciled": abs(summary.closing_balance - control_balance) < 0.01,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
