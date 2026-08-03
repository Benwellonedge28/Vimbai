"""
Vimbai Bank Reconciliation Service
Complete bank reconciliation process including statement import, matching, and adjustments.
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

SERVICE_NAME = "bank-reconciliation-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8041"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Bank Reconciliation Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class TransactionType(str, Enum):
    DEPOSIT = "deposit"
    WITHDRAWAL = "withdrawal"
    CHEQUE = "cheque"
    DIRECT_DEBIT = "direct_debit"
    DIRECT_CREDIT = "direct_credit"
    TRANSFER = "transfer"
    BANK_CHARGE = "bank_charge"
    INTEREST = "interest"
    STANDING_ORDER = "standing_order"


class MatchStatus(str, Enum):
    MATCHED = "matched"
    UNMATCHED = "unmatched"
    PARTIAL_MATCH = "partial_match"
    DISPUTED = "disputed"


class BankStatementLine(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    statement_id: str
    date: datetime
    description: str
    reference: Optional[str] = None
    transaction_type: TransactionType
    amount: float
    balance: float
    is_debit: bool
    matched: bool = False
    matched_transaction_id: Optional[str] = None


class BankStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    bank_account: str
    statement_number: str
    statement_start_date: datetime
    statement_end_date: datetime
    opening_balance: float
    closing_balance: float
    total_credits: float = 0
    total_debits: float = 0
    lines: List[BankStatementLine] = []
    status: str = "imported"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CashBookEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    date: datetime
    description: str
    reference: str
    transaction_type: str
    amount: float
    is_debit: bool
    bank_reconciliation_id: Optional[str] = None
    matched: bool = False
    matched_statement_id: Optional[str] = None


class ReconciliationItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    item_type: str  # "bank_only" or "cash_book_only"
    date: datetime
    description: str
    reference: Optional[str] = None
    amount: float
    match_status: MatchStatus = MatchStatus.UNMATCHED
    suggested_match_id: Optional[str] = None
    notes: Optional[str] = None


class BankReconciliation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    bank_account: str
    reconciliation_date: datetime
    statement_balance: float
    cash_book_balance: float
    difference: float = 0

    # Adjustments
    outstanding_deposits: float = 0
    outstanding_cheques: float = 0
    bank_errors: float = 0
    cash_book_errors: float = 0
    unpresented_cheques: float = 0
    uncredited_deposits: float = 0

    # Final balances
    adjusted_statement_balance: float = 0
    adjusted_cash_book_balance: float = 0

    items: List[ReconciliationItem] = []
    status: str = "in_progress"
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    completed_at: Optional[datetime] = None


# In-memory storage
bank_statements: List[BankStatement] = []
cash_book_entries: List[CashBookEntry] = []
reconciliations: List[BankReconciliation] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Bank reconciliation service"}


# ============================================================================
# Bank Statement Management
# ============================================================================

@app.post("/statements")
async def import_statement(data: BankStatement):
    """Import bank statement."""
    data.id = str(uuid.uuid4())
    data.created_at = datetime.utcnow()

    # Calculate totals
    data.total_credits = sum(l.amount for l in data.lines if not l.is_debit)
    data.total_debits = sum(l.amount for l in data.lines if l.is_debit)

    bank_statements.append(data)
    await call_audit_service("IMPORT", "statement", data.id, {"lines": len(data.lines)})
    return data


@app.get("/statements")
async def list_statements(bank_account: Optional[str] = None):
    """List bank statements."""
    result = bank_statements
    if bank_account:
        result = [s for s in result if s.bank_account == bank_account]
    return {"statements": result}


@app.get("/statements/{statement_id}")
async def get_statement(statement_id: str):
    """Get statement details."""
    stmt = next((s for s in bank_statements if s.id == statement_id), None)
    if not stmt:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Statement not found")
    return stmt


# ============================================================================
# Cash Book Management
# ============================================================================

@app.post("/cash-book")
async def add_cash_book_entry(data: CashBookEntry):
    """Add cash book entry."""
    data.id = str(uuid.uuid4())
    cash_book_entries.append(data)
    await call_audit_service("CREATE", "cash_book", data.id, {"amount": data.amount})
    return data


@app.get("/cash-book")
async def list_cash_book_entries(
    bank_account: str, start_date: Optional[datetime] = None, end_date: Optional[datetime] = None,
    matched: Optional[bool] = None
):
    """List cash book entries."""
    result = [e for e in cash_book_entries if e.date.year == datetime.utcnow().year]
    if start_date:
        result = [e for e in result if e.date >= start_date]
    if end_date:
        result = [e for e in result if e.date <= end_date]
    if matched is not None:
        result = [e for e in result if e.matched == matched]
    return {"entries": result}


# ============================================================================
# Reconciliation Process
# ============================================================================

@app.post("/reconcile")
async def create_reconciliation(
    bank_account: str, reconciliation_date: datetime,
    statement_balance: float, cash_book_balance: float,
    outstanding_deposits: float = 0, outstanding_cheques: float = 0,
    bank_errors: float = 0, cash_book_errors: float = 0
):
    """Create bank reconciliation."""
    reconciliation = BankReconciliation(
        bank_account=bank_account, reconciliation_date=reconciliation_date,
        statement_balance=statement_balance, cash_book_balance=cash_book_balance,
        outstanding_deposits=outstanding_deposits, outstanding_cheques=outstanding_cheques,
        bank_errors=bank_errors, cash_book_errors=cash_book_errors
    )

    # Calculate adjusted balances
    reconciliation.adjusted_statement_balance = (
        statement_balance - outstanding_cheques + outstanding_deposits - bank_errors
    )
    reconciliation.adjusted_cash_book_balance = (
        cash_book_balance - bank_errors + cash_book_errors
    )
    reconciliation.difference = (
        reconciliation.adjusted_statement_balance - reconciliation.adjusted_cash_book_balance
    )

    reconciliations.append(reconciliation)
    await call_audit_service("CREATE", "reconciliation", reconciliation.id, {"difference": reconciliation.difference})
    return reconciliation


@app.post("/reconcile/{reconciliation_id}/auto-match")
async def auto_match_transactions(reconciliation_id: str, statement_id: str):
    """Auto-match transactions between statement and cash book."""
    reconciliation = next((r for r in reconciliations if r.id == reconciliation_id), None)
    if not reconciliation:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Reconciliation not found")

    statement = next((s for s in bank_statements if s.id == statement_id), None)
    if not statement:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Statement not found")

    # Simple matching by reference and amount
    unmatched_items = []

    for line in statement.lines:
        if line.matched:
            continue

        # Try to find matching cash book entry
        for entry in cash_book_entries:
            if entry.matched:
                continue

            # Match by reference
            if line.reference and entry.reference and line.reference == entry.reference:
                if abs(line.amount - entry.amount) < 0.01:
                    line.matched = True
                    line.matched_transaction_id = entry.id
                    entry.matched = True
                    entry.matched_statement_id = line.id

    # Identify unmatched items
    unmatched_bank = [l for l in statement.lines if not l.matched]
    unmatched_cash = [e for e in cash_book_entries if not e.matched]

    reconciliation.items = [
        ReconciliationItem(item_type="bank_only", date=l.date, description=l.description,
                         reference=l.reference, amount=l.amount)
        for l in unmatched_bank
    ] + [
        ReconciliationItem(item_type="cash_book_only", date=e.date, description=e.description,
                         reference=e.reference, amount=e.amount)
        for e in unmatched_cash
    ]

    return {
        "reconciliation": reconciliation,
        "matched_count": len(statement.lines) - len(unmatched_bank),
        "unmatched_bank": len(unmatched_bank),
        "unmatched_cash_book": len(unmatched_cash)
    }


@app.post("/reconcile/{reconciliation_id}/post-adjustments")
async def post_adjustments(reconciliation_id: str):
    """Post adjustment journal entries."""
    reconciliation = next((r for r in reconciliations if r.id == reconciliation_id), None)
    if not reconciliation:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Reconciliation not found")

    entries = []

    # Bank errors
    if reconciliation.bank_errors != 0:
        entries.append({
            "account_code": "2100" if reconciliation.bank_errors > 0 else "1100",
            "description": "Bank reconciliation adjustment",
            "debit": abs(reconciliation.bank_errors) if reconciliation.bank_errors < 0 else 0,
            "credit": abs(reconciliation.bank_errors) if reconciliation.bank_errors > 0 else 0,
        })

    journal_entry = {
        "date": reconciliation.reconciliation_date,
        "description": f"Bank reconciliation adjustments - {reconciliation.reconciliation_date.date()}",
        "entries": entries,
        "reference": f"BANK-REC-{reconciliation.id[:8]}",
    }

    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    reconciliation.journal_entry_id = result.get("id")
    reconciliation.status = "completed"
    reconciliation.completed_at = datetime.utcnow()

    await call_audit_service("COMPLETE", "reconciliation", reconciliation_id, {"journal_id": result.get("id")})
    return reconciliation


@app.get("/reconciliations")
async def list_reconciliations(bank_account: Optional[str] = None, status: Optional[str] = None):
    """List reconciliations."""
    result = reconciliations
    if bank_account:
        result = [r for r in result if r.bank_account == bank_account]
    if status:
        result = [r for r in result if r.status == status]
    return {"reconciliations": result[-20:]}


@app.get("/reconciliations/{reconciliation_id}")
async def get_reconciliation(reconciliation_id: str):
    """Get reconciliation details."""
    recon = next((r for r in reconciliations if r.id == reconciliation_id), None)
    if not recon:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Reconciliation not found")
    return recon


# ============================================================================
# Outstanding Items Report
# ============================================================================

@app.get("/outstanding-items")
async def get_outstanding_items(bank_account: str, as_of_date: Optional[datetime] = None):
    """Get outstanding cheques and deposits."""
    as_of_date = as_of_date or datetime.utcnow()

    # Get latest reconciliation
    latest = next((r for r in reversed(reconciliations) if r.bank_account == bank_account), None)

    if not latest:
        return {"outstanding_cheques": [], "outstanding_deposits": [], "total_cheques": 0, "total_deposits": 0}

    return {
        "outstanding_cheques": [
            {"date": i.date, "description": i.description, "amount": i.amount, "reference": i.reference}
            for i in latest.items if i.amount > 0 and "cheque" in i.description.lower()
        ],
        "outstanding_deposits": [
            {"date": i.date, "description": i.description, "amount": i.amount, "reference": i.reference}
            for i in latest.items if i.amount < 0 or "deposit" in i.description.lower()
        ],
        "total_cheques": latest.outstanding_cheques,
        "total_deposits": latest.outstanding_deposits
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)