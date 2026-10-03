"""
Vimbai Sales Ledger Control Service
Manages purchases ledger control account and debtor transactions.

Transactions persist in Neo4j, stamped with the caller (X-User-Id) and the
Book context (X-Book-ID, verified upstream by the API gateway). Balances,
summaries, and reconciliation are derived from the caller's own
Book-visible transactions only. Original status codes and response shapes
are preserved; the accounting/audit side-calls keep their fail-soft
behavior.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
from datetime import datetime, timezone
from typing import Any, Dict, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "sales_ledger_control_service" not in _sys.modules or not hasattr(
    _sys.modules.get("sales_ledger_control_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("sales_ledger_control_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["sales_ledger_control_service"] = _pkg
    _sys.modules["sales_ledger_control_service"].__path__ = [_HERE]

import httpx
import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from neo4j import AsyncSession
from sales_ledger_control_service import crud
from sales_ledger_control_service.database import Neo4jConnector
from sales_ledger_control_service.dependencies import book_id_var, get_db_session, get_user_id
from sales_ledger_control_service.exceptions import SalesLedgerControlError
from sales_ledger_control_service.models import ControlAccountSummary, DebtorTransaction, TransactionType

SERVICE_NAME = "sales-ledger-control-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8038"))
AUDIT_SERVICE_URL = _os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = _os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(SalesLedgerControlError)
async def _plc_error(request: Request, exc: SalesLedgerControlError):
    return JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
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
                    "timestamp": datetime.now(timezone.utc).isoformat(),
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
        "description": "Purchases ledger control account management",
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
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
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

    # Stamp the running balance (replayed from the caller's visible ledger).
    balances = crud.derive_balances(await crud.list_transactions(db_session, user_id))
    current_balance = balances.get(debtor_id, 0.0)
    if transaction_type in [TransactionType.INVOICE]:
        txn.balance = current_balance + amount
    elif transaction_type in [
        TransactionType.CREDIT_NOTE,
        TransactionType.PAYMENT,
        TransactionType.REFUND,
        TransactionType.BAD_DEBT,
    ]:
        txn.balance = current_balance - amount

    txn = await crud.create_transaction(db_session, user_id, txn)

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
        if result.get("id"):
            txn.journal_entry_id = result.get("id")
            await crud.update_journal_entry_id(db_session, user_id, txn.id, txn.journal_entry_id)

    await call_audit_service("CREATE", "transaction", txn.id, {"type": transaction_type, "amount": amount})
    return txn


@app.get("/control-account/summary")
async def get_control_summary(
    as_of_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get control account summary."""
    as_of_date = as_of_date or datetime.now(timezone.utc)
    if as_of_date.tzinfo is None:
        as_of_date = as_of_date.replace(tzinfo=timezone.utc)
    transactions = [t for t in await crud.list_transactions(db_session, user_id) if t.date <= as_of_date]

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
async def get_debtor_balance(
    debtor_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get an individual debtor balance (caller-scoped)."""
    transactions = [t for t in await crud.list_transactions(db_session, user_id) if t.debtor_id == debtor_id]
    balances = crud.derive_balances(await crud.list_transactions(db_session, user_id))
    balance = balances.get(debtor_id, 0)

    return {
        "debtor_id": debtor_id,
        "balance": balance,
        "transaction_count": len(transactions),
        "transactions": transactions[-10:],
    }


@app.get("/debtors")
async def list_debtors(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's debtors with balances."""
    balances = crud.derive_balances(await crud.list_transactions(db_session, user_id))
    return {
        "debtors": [{"debtor_id": cid, "balance": bal} for cid, bal in balances.items()],
        "total_balance": sum(balances.values()),
    }


@app.post("/reconcile")
async def reconcile_control_account(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Reconcile control account."""
    balances = crud.derive_balances(await crud.list_transactions(db_session, user_id))
    control_balance = sum(balances.values())
    summary = await get_control_summary(user_id=user_id, db_session=db_session)
    return {
        "control_account_balance": summary.closing_balance,
        "sales_ledger_total": control_balance,
        "difference": abs(summary.closing_balance - control_balance),
        "reconciled": abs(summary.closing_balance - control_balance) < 0.01,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
