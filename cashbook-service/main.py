# This file may be imported bare (Docker `uvicorn main:app`, bracket mounts), so it
# bootstraps its own package alias before importing sibling modules.
import importlib.util as _ilu
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
_PKG = "cashbook_service"
if _PKG not in _sys.modules or not hasattr(_sys.modules.get(_PKG), "__path__"):
    _spec = _ilu.spec_from_file_location(_PKG, _os.path.join(_HERE, "__init__.py"))
    _pkg = _ilu.module_from_spec(_spec)
    _pkg.__path__ = [_HERE]
    _sys.modules[_PKG] = _pkg

"""
Vimbai Cash Book Service
Complete cash book management with multi-currency support,
bank reconciliation, and cash flow tracking.

Neo4j-backed, user-owned and Book-scoped (X-User-Id / X-Book-ID headers).
"""

from datetime import datetime, timezone
from decimal import Decimal
from typing import Dict, List, Optional

from cashbook_service import crud, models
from cashbook_service.database import Neo4jConnector
from cashbook_service.dependencies import book_id_var, get_db_session, get_user_id
from cashbook_service.exceptions import CashBookError, ConflictError, NotFoundError, ValidationError
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.responses import JSONResponse
from neo4j import AsyncSession

app = FastAPI(
    title="Vimbai Cash Book Service",
    description="Comprehensive cash book management with multi-currency support, bank reconciliation, and cash flow tracking",
    version="1.0.0",
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
        _os.getenv("NEO4J_URI", "bolt://localhost:7687"),
        _os.getenv("NEO4J_USER", "neo4j"),
        _os.getenv("NEO4J_PASSWORD", "password"),
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


# ============================================================================
# API Endpoints
# ============================================================================


@app.get("/")
async def health_check():
    """Health check endpoint"""
    return {
        "status": "healthy",
        "service": "cashbook",
        "version": "1.0.0",
    }


# --- Bank Account Management ---


@app.post("/accounts", response_model=models.BankAccount)
async def create_bank_account(
    account: models.BankAccount,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a new bank/cash account"""
    return await crud.create_bank_account(db_session, user_id, account)


@app.get("/accounts", response_model=List[models.BankAccount])
async def list_bank_accounts(
    account_type: Optional[models.BankAccountType] = None,
    currency: Optional[str] = None,
    is_active: Optional[bool] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all bank accounts"""
    results = await crud.list_bank_accounts(db_session, user_id)

    if account_type:
        results = [a for a in results if a.account_type == account_type]
    if currency:
        results = [a for a in results if a.currency == currency]
    if is_active is not None:
        results = [a for a in results if a.is_active == is_active]

    return results


@app.get("/accounts/{account_id}", response_model=models.BankAccount)
async def get_bank_account(
    account_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get bank account details"""
    account = await crud.get_bank_account(db_session, user_id, account_id)
    if not account:
        raise HTTPException(status_code=404, detail="Account not found")
    return account


@app.put("/accounts/{account_id}", response_model=models.BankAccount)
async def update_bank_account(
    account_id: str,
    account: models.BankAccount,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update bank account"""
    return await crud.update_bank_account(db_session, user_id, account_id, account)


# --- Cash Book Entry Management ---


@app.post("/entries", response_model=models.CashBookEntry)
async def create_cash_book_entry(
    entry: models.CashBookEntry,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a cash book entry"""
    return await crud.create_cash_book_entry(db_session, user_id, entry)


@app.get("/entries", response_model=List[models.CashBookEntry])
async def list_cash_book_entries(
    book_type: Optional[models.CashBookType] = None,
    bank_account: Optional[str] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    status: Optional[models.TransactionStatus] = None,
    reconciled: Optional[bool] = None,
    limit: int = 100,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List cash book entries with filters"""
    results = await crud.list_cash_book_entries(db_session, user_id, limit=limit)

    if book_type:
        results = [e for e in results if e.book_type == book_type]
    if bank_account:
        results = [e for e in results if e.bank_account == bank_account]
    if start_date:
        results = [e for e in results if e.entry_date >= start_date]
    if end_date:
        results = [e for e in results if e.entry_date <= end_date]
    if status:
        results = [e for e in results if e.status == status]
    if reconciled is not None:
        results = [e for e in results if e.reconciled == reconciled]

    results.sort(key=lambda x: x.entry_date, reverse=True)
    return results


@app.get("/entries/{entry_id}", response_model=models.CashBookEntry)
async def get_cash_book_entry(
    entry_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get cash book entry details"""
    entry = await crud.get_cash_book_entry(db_session, user_id, entry_id)
    if not entry:
        raise HTTPException(status_code=404, detail="Entry not found")
    return entry


@app.put("/entries/{entry_id}/post", response_model=models.CashBookEntry)
async def post_cash_book_entry(
    entry_id: str,
    posted_by: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Post a cash book entry"""
    entry = await crud.get_cash_book_entry(db_session, user_id, entry_id)
    if not entry:
        raise HTTPException(status_code=404, detail="Entry not found")

    return await crud.set_entry_status(db_session, user_id, entry_id, models.TransactionStatus.POSTED)


@app.put("/entries/{entry_id}/void")
async def void_cash_book_entry(
    entry_id: str,
    voided_by: str,
    reason: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Void a cash book entry"""
    entry = await crud.get_cash_book_entry(db_session, user_id, entry_id)
    if not entry:
        raise HTTPException(status_code=404, detail="Entry not found")

    # Reverse the balance change
    if entry.bank_account:
        account = await crud.get_bank_account_by_code(db_session, user_id, entry.bank_account)
        if account:
            if entry.is_debit:
                account.current_balance -= entry.base_amount
            else:
                account.current_balance += entry.base_amount
            await crud.adjust_account_balance(db_session, user_id, account.id, account.current_balance)

    await crud.set_entry_status(db_session, user_id, entry_id, models.TransactionStatus.VOIDED)

    return {"status": "voided", "entry_id": entry_id, "reason": reason}


# --- Cash Book Summary ---


@app.get("/summary/{account_code}", response_model=models.CashBookSummary)
async def get_cash_book_summary(
    account_code: str,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get cash book summary for an account"""
    account = await crud.get_bank_account_by_code(db_session, user_id, account_code)
    if not account:
        raise HTTPException(status_code=404, detail="Account not found")

    entries = [
        e for e in await crud.list_cash_book_entries(db_session, user_id, limit=10000) if e.bank_account == account_code
    ]

    if start_date:
        entries = [e for e in entries if e.entry_date >= start_date]
    if end_date:
        entries = [e for e in entries if e.entry_date <= end_date]

    total_debits = sum(e.base_amount for e in entries if e.is_debit and e.status == models.TransactionStatus.POSTED)
    total_credits = sum(
        e.base_amount for e in entries if not e.is_debit and e.status == models.TransactionStatus.POSTED
    )

    last_transaction = max((e.entry_date for e in entries), default=None)

    return models.CashBookSummary(
        account_code=account_code,
        account_name=account.account_name,
        opening_balance=account.opening_balance,
        total_debits=total_debits,
        total_credits=total_credits,
        closing_balance=account.opening_balance + total_debits - total_credits,
        transaction_count=len(entries),
        last_transaction_date=last_transaction,
    )


# --- Bank Reconciliation ---


@app.post("/reconciliations", response_model=models.BankReconciliation)
async def create_reconciliation(
    reconciliation: models.BankReconciliation,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create bank reconciliation"""
    return await crud.create_reconciliation(db_session, user_id, reconciliation)


@app.get("/reconciliations", response_model=List[models.BankReconciliation])
async def list_reconciliations(
    bank_account: Optional[str] = None,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List bank reconciliations"""
    results = await crud.list_reconciliations(db_session, user_id)

    if bank_account:
        results = [r for r in results if r.bank_account == bank_account]
    if status:
        results = [r for r in results if r.status == status]

    results.sort(key=lambda x: x.statement_date, reverse=True)
    return results


@app.get("/reconciliations/{reconciliation_id}", response_model=models.BankReconciliation)
async def get_reconciliation(
    reconciliation_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get reconciliation details"""
    reconciliation = await crud.get_reconciliation(db_session, user_id, reconciliation_id)
    if not reconciliation:
        raise HTTPException(status_code=404, detail="Reconciliation not found")
    return reconciliation


@app.put("/reconciliations/{reconciliation_id}/complete", response_model=models.BankReconciliation)
async def complete_reconciliation(
    reconciliation_id: str,
    reviewed_by: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Complete a reconciliation"""
    return await crud.complete_reconciliation(db_session, user_id, reconciliation_id, reviewed_by)


@app.get("/reconciliations/{reconciliation_id}/outstanding")
async def get_outstanding_items(
    reconciliation_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get outstanding items for reconciliation"""
    reconciliation = await crud.get_reconciliation(db_session, user_id, reconciliation_id)
    if not reconciliation:
        raise HTTPException(status_code=404, detail="Reconciliation not found")

    entries = [
        e
        for e in await crud.list_cash_book_entries(db_session, user_id, limit=10000)
        if e.bank_account == reconciliation.bank_account
        and not e.reconciled
        and e.status == models.TransactionStatus.POSTED
    ]

    total_debits = sum(e.base_amount for e in entries if e.is_debit)
    total_credits = sum(e.base_amount for e in entries if not e.is_debit)

    return {
        "reconciliation_id": reconciliation_id,
        "total_items": len(entries),
        "total_debits": str(total_debits),
        "total_credits": str(total_credits),
        "difference": str(total_debits - total_credits),
        "entries": [e.model_dump(mode="json") for e in entries],
    }


# --- Cash Flow ---


@app.post("/cash-flow", response_model=models.CashFlowEntry)
async def create_cash_flow_entry(
    entry: models.CashFlowEntry,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create cash flow entry"""
    return await crud.create_cash_flow_entry(db_session, user_id, entry)


@app.get("/cash-flow", response_model=List[models.CashFlowEntry])
async def list_cash_flow_entries(
    cash_flow_type: Optional[str] = None,
    category: Optional[str] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List cash flow entries"""
    results = await crud.list_cash_flow_entries(db_session, user_id, limit=limit)

    if cash_flow_type:
        results = [e for e in results if e.cash_flow_type == cash_flow_type]
    if category:
        results = [e for e in results if e.category == category]
    if start_date:
        results = [e for e in results if e.entry_date >= start_date]
    if end_date:
        results = [e for e in results if e.entry_date <= end_date]

    results.sort(key=lambda x: x.entry_date, reverse=True)
    return results


@app.get("/cash-flow/summary")
async def get_cash_flow_summary(
    start_date: datetime,
    end_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get cash flow summary for period"""
    entries = [
        e
        for e in await crud.list_cash_flow_entries(db_session, user_id, limit=10000)
        if start_date <= e.entry_date <= end_date
    ]

    operating = [e for e in entries if e.cash_flow_type == "operating"]
    investing = [e for e in entries if e.cash_flow_type == "investing"]
    financing = [e for e in entries if e.cash_flow_type == "financing"]

    def calculate_total(entries_list):
        actual = sum(e.actual_amount for e in entries_list if e.actual_amount)
        expected = sum(e.expected_amount for e in entries_list if e.expected_amount)
        return {
            "count": len(entries_list),
            "expected_total": str(expected),
            "actual_total": str(actual),
            "variance": str(expected - actual),
        }

    return {
        "period": {"start": start_date, "end": end_date},
        "operating": calculate_total(operating),
        "investing": calculate_total(investing),
        "financing": calculate_total(financing),
        "net_change": str(sum(e.actual_amount or 0 for e in entries) - sum(e.expected_amount or 0 for e in entries)),
    }


# --- Cash Position ---


@app.get("/cash-position", response_model=models.CashPosition)
async def get_cash_position(
    as_of_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get current cash position"""
    if as_of_date is None:
        as_of_date = datetime.now(timezone.utc)

    total_cash = Decimal("0")
    total_bank = Decimal("0")
    total_savings = Decimal("0")
    total_current = Decimal("0")
    by_currency: Dict[str, Decimal] = {}
    by_account = []

    for account in await crud.list_bank_accounts(db_session, user_id):
        balance = account.current_balance

        by_account.append(
            {
                "account_code": account.account_code,
                "account_name": account.account_name,
                "account_type": account.account_type.value,
                "balance": str(balance),
                "currency": account.currency,
            }
        )

        if account.account_type == models.BankAccountType.CASH:
            total_cash += balance
        elif account.account_type == models.BankAccountType.BANK:
            total_bank += balance
        elif account.account_type == models.BankAccountType.SAVINGS:
            total_savings += balance
        elif account.account_type == models.BankAccountType.CURRENT:
            total_current += balance

        by_currency.setdefault(account.currency, Decimal("0"))
        by_currency[account.currency] += balance

    return models.CashPosition(
        as_of_date=as_of_date,
        total_cash=total_cash,
        total_bank=total_bank,
        total_savings=total_savings,
        total_current=total_current,
        by_currency={k: str(v) for k, v in by_currency.items()},
        by_account=by_account,
    )


@app.get("/cash-position/history")
async def get_cash_position_history(
    start_date: datetime,
    end_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get cash position history"""
    entries = [
        e
        for e in await crud.list_cash_book_entries(db_session, user_id, limit=10000)
        if start_date <= e.entry_date <= end_date
    ]

    daily_positions = {}
    for entry in entries:
        date_key = entry.entry_date.date().isoformat()
        if date_key not in daily_positions:
            daily_positions[date_key] = {"debits": Decimal("0"), "credits": Decimal("0")}

        if entry.is_debit:
            daily_positions[date_key]["debits"] += entry.base_amount
        else:
            daily_positions[date_key]["credits"] += entry.base_amount

    history = []
    running_balance = Decimal("0")
    for date_key in sorted(daily_positions.keys()):
        day = daily_positions[date_key]
        net = day["debits"] - day["credits"]
        running_balance += net
        history.append(
            {
                "date": date_key,
                "total_debits": str(day["debits"]),
                "total_credits": str(day["credits"]),
                "net_change": str(net),
                "running_balance": str(running_balance),
            }
        )

    return {"start_date": start_date, "end_date": end_date, "history": history}
