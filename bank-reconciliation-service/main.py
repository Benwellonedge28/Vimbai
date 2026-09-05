# This file may be imported bare (Docker `uvicorn main:app`, bracket mounts), so it
# bootstraps its own package alias before importing sibling modules.
import importlib.util as _ilu
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
_PKG = "bank_reconciliation_service"
if _PKG not in _sys.modules or not hasattr(_sys.modules.get(_PKG), "__path__"):
    _spec = _ilu.spec_from_file_location(_PKG, _os.path.join(_HERE, "__init__.py"))
    _pkg = _ilu.module_from_spec(_spec)
    _pkg.__path__ = [_HERE]
    _sys.modules[_PKG] = _pkg

"""
Vimbai Bank Reconciliation Service
Complete bank reconciliation process including statement import, matching, and adjustments.

Neo4j-backed, user-owned and Book-scoped (X-User-Id / X-Book-ID headers).
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from bank_reconciliation_service import crud, models
from bank_reconciliation_service.database import Neo4jConnector
from bank_reconciliation_service.dependencies import book_id_var, get_db_session, get_user_id
from bank_reconciliation_service.exceptions import (
    BankReconciliationError,
    ConflictError,
    NotFoundError,
    ValidationError,
)
from fastapi import Depends, FastAPI, HTTPException, Request, status
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from neo4j import AsyncSession

SERVICE_NAME = "bank-reconciliation-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8041"))
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

app = FastAPI(title="Vimbai Bank Reconciliation Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Capture the Book context for the duration of the request."""
    token = book_id_var.set(request.headers.get("x-book-id"))
    try:
        return await call_next(request)
    finally:
        book_id_var.reset(token)


@app.on_event("startup")
async def startup():
    Neo4jConnector.configure(
        os.getenv("NEO4J_URI", "bolt://localhost:7687"),
        os.getenv("NEO4J_USER", "neo4j"),
        os.getenv("NEO4J_PASSWORD", "password"),
    )


@app.on_event("shutdown")
async def shutdown():
    await Neo4jConnector.close_driver()


@app.exception_handler(NotFoundError)
async def not_found_exception_handler(request: Request, exc: NotFoundError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


@app.exception_handler(ConflictError)
async def conflict_exception_handler(request: Request, exc: ConflictError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


@app.exception_handler(ValidationError)
async def validation_exception_handler(request: Request, exc: ValidationError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Bank reconciliation service"}


# ============================================================================
# Bank Statement Management
# ============================================================================


@app.post("/statements")
async def import_statement(
    data: models.BankStatement,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Import bank statement."""
    statement = await crud.create_statement(db_session, user_id, data)
    await call_audit_service("IMPORT", "statement", statement.id, {"lines": len(statement.lines)})
    return statement


@app.get("/statements")
async def list_statements(
    bank_account: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List bank statements."""
    result = await crud.list_statements(db_session, user_id)
    if bank_account:
        result = [s for s in result if s.bank_account == bank_account]
    return {"statements": result}


@app.get("/statements/{statement_id}")
async def get_statement(
    statement_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get statement details."""
    stmt = await crud.get_statement(db_session, user_id, statement_id)
    if not stmt:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Statement not found")
    return stmt


# ============================================================================
# Cash Book Management
# ============================================================================


@app.post("/cash-book")
async def add_cash_book_entry(
    data: models.CashBookEntry,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add cash book entry."""
    entry = await crud.create_cash_book_entry(db_session, user_id, data)
    await call_audit_service("CREATE", "cash_book", entry.id, {"amount": entry.amount})
    return entry


@app.get("/cash-book")
async def list_cash_book_entries(
    bank_account: str,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    matched: Optional[bool] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List cash book entries."""
    result = await crud.list_cash_book_entries(db_session, user_id)
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
    bank_account: str,
    reconciliation_date: datetime,
    statement_balance: float,
    cash_book_balance: float,
    outstanding_deposits: float = 0,
    outstanding_cheques: float = 0,
    bank_errors: float = 0,
    cash_book_errors: float = 0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create bank reconciliation."""
    reconciliation = await crud.create_reconciliation(
        db_session,
        user_id,
        bank_account=bank_account,
        reconciliation_date=reconciliation_date,
        statement_balance=statement_balance,
        cash_book_balance=cash_book_balance,
        outstanding_deposits=outstanding_deposits,
        outstanding_cheques=outstanding_cheques,
        bank_errors=bank_errors,
        cash_book_errors=cash_book_errors,
    )
    await call_audit_service("CREATE", "reconciliation", reconciliation.id, {"difference": reconciliation.difference})
    return reconciliation


@app.post("/reconcile/{reconciliation_id}/auto-match")
async def auto_match_transactions(
    reconciliation_id: str,
    statement_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Auto-match transactions between statement and cash book."""
    reconciliation = await crud.get_reconciliation(db_session, user_id, reconciliation_id)
    if not reconciliation:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Reconciliation not found")

    statement = await crud.get_statement(db_session, user_id, statement_id)
    if not statement:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Statement not found")

    entries = await crud.list_cash_book_entries(db_session, user_id)

    # Simple matching by reference and amount
    for line in statement.lines:
        if line.matched:
            continue

        # Try to find matching cash book entry
        for entry in entries:
            if entry.matched:
                continue

            # Match by reference
            if line.reference and entry.reference and line.reference == entry.reference:
                if abs(line.amount - entry.amount) < 0.01:
                    line.matched = True
                    line.matched_transaction_id = entry.id
                    entry.matched = True
                    entry.matched_statement_id = line.id
                    entry.bank_reconciliation_id = reconciliation.id

    # Persist match mutations
    await crud.save_statement_lines(db_session, user_id, statement)
    for entry in entries:
        if entry.matched:
            await crud.save_cash_book_entry(db_session, user_id, entry)

    # Identify unmatched items
    unmatched_bank = [line for line in statement.lines if not line.matched]
    unmatched_cash = [entry for entry in entries if not entry.matched]

    reconciliation.items = [
        models.ReconciliationItem(
            item_type="bank_only",
            date=line.date,
            description=line.description,
            reference=line.reference,
            amount=line.amount,
        )
        for line in unmatched_bank
    ] + [
        models.ReconciliationItem(
            item_type="cash_book_only",
            date=entry.date,
            description=entry.description,
            reference=entry.reference,
            amount=entry.amount,
        )
        for entry in unmatched_cash
    ]
    await crud.save_reconciliation(db_session, user_id, reconciliation)

    return {
        "reconciliation": reconciliation,
        "matched_count": len(statement.lines) - len(unmatched_bank),
        "unmatched_bank": len(unmatched_bank),
        "unmatched_cash_book": len(unmatched_cash),
    }


@app.post("/reconcile/{reconciliation_id}/post-adjustments")
async def post_adjustments(
    reconciliation_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Post adjustment journal entries."""
    reconciliation = await crud.get_reconciliation(db_session, user_id, reconciliation_id)
    if not reconciliation:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Reconciliation not found")

    entries = []

    # Bank errors
    if reconciliation.bank_errors != 0:
        entries.append(
            {
                "account_code": "2100" if reconciliation.bank_errors > 0 else "1100",
                "description": "Bank reconciliation adjustment",
                "debit": abs(reconciliation.bank_errors) if reconciliation.bank_errors < 0 else 0,
                "credit": abs(reconciliation.bank_errors) if reconciliation.bank_errors > 0 else 0,
            }
        )

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

    await crud.save_reconciliation(db_session, user_id, reconciliation)

    await call_audit_service("COMPLETE", "reconciliation", reconciliation_id, {"journal_id": result.get("id")})
    return reconciliation


@app.get("/reconciliations")
async def list_reconciliations(
    bank_account: Optional[str] = None,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List reconciliations."""
    result = await crud.list_reconciliations(db_session, user_id)
    if bank_account:
        result = [r for r in result if r.bank_account == bank_account]
    if status:
        result = [r for r in result if r.status == status]
    return {"reconciliations": result[-20:]}


@app.get("/reconciliations/{reconciliation_id}")
async def get_reconciliation(
    reconciliation_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get reconciliation details."""
    recon = await crud.get_reconciliation(db_session, user_id, reconciliation_id)
    if not recon:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Reconciliation not found")
    return recon


# ============================================================================
# Outstanding Items Report
# ============================================================================


@app.get("/outstanding-items")
async def get_outstanding_items(
    bank_account: str,
    as_of_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get outstanding cheques and deposits."""
    as_of_date = as_of_date or datetime.utcnow()

    # Get latest reconciliation for this account
    all_recons = await crud.list_reconciliations(db_session, user_id)
    latest = next((r for r in reversed(all_recons) if r.bank_account == bank_account), None)

    if not latest:
        return {"outstanding_cheques": [], "outstanding_deposits": [], "total_cheques": 0, "total_deposits": 0}

    return {
        "outstanding_cheques": [
            {"date": i.date, "description": i.description, "amount": i.amount, "reference": i.reference}
            for i in latest.items
            if i.amount > 0 and "cheque" in i.description.lower()
        ],
        "outstanding_deposits": [
            {"date": i.date, "description": i.description, "amount": i.amount, "reference": i.reference}
            for i in latest.items
            if i.amount < 0 or "deposit" in i.description.lower()
        ],
        "total_cheques": latest.outstanding_cheques,
        "total_deposits": latest.outstanding_deposits,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
