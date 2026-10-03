"""Vimbai Company Accounting Service - company registry, share capital, dividends. Port: 8101

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "company_accounting_service" not in _sys.modules or not hasattr(
    _sys.modules.get("company_accounting_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("company_accounting_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["company_accounting_service"] = _pkg
    _sys.modules["company_accounting_service"].__path__ = [_HERE]

import os
import uuid
from datetime import datetime, timezone
from decimal import Decimal
from typing import Any, Dict, List, Optional

import httpx
import structlog
from company_accounting_service import crud, models
from company_accounting_service.dependencies import book_id_var, get_db_session, get_user_id
from company_accounting_service.exceptions import CompanyAccountingError
from company_accounting_service.models import (
    CapitalTransaction,
    CapitalTransactionType,
    Company,
    CompanyStatus,
    CompanyType,
    Dividend,
    DividendPayment,
    DividendType,
    Reserve,
    RetainedEarnings,
    ShareCapital,
    ShareClass,
    Shareholder,
)
from fastapi import Depends, FastAPI, HTTPException, Request
from neo4j import AsyncSession

SERVICE_NAME = "company-accounting-service"
PORT = int(os.getenv("PORT", "8101"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(
    title="Vimbai Company Accounting Service",
    description="Company-level financial management: shareholder equity, dividends, capital transactions, company-specific reporting",
    version="2.0.0",
)

# Distributed tracing (OpenTelemetry)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(CompanyAccountingError)
async def _company_accounting_error(request: Request, exc: CompanyAccountingError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


# ============================================================================
# Configuration - Internal API endpoints
# ============================================================================

ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8091")


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None):
    """Call accounting service for core accounting functions (failures tolerated)"""
    async with httpx.AsyncClient() as client:
        url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
        try:
            if method == "GET":
                response = await client.get(url, timeout=10.0)
            elif method == "POST":
                response = await client.post(url, json=data, timeout=10.0)
            else:
                return {"error": "Method not supported"}
            return response.json()
        except httpx.RequestError:
            return {"error": "Accounting service unavailable", "data": None}


async def call_audit_service(event_data: Dict):
    """Log to audit service (failures tolerated)"""
    async with httpx.AsyncClient() as client:
        url = f"{AUDIT_SERVICE_URL}/events"
        try:
            await client.post(url, json=event_data, timeout=5.0)
        except httpx.RequestError:
            pass


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


# ============================================================================
# Company Management
# ============================================================================


@app.post("/companies")
async def create_company(
    company: Company,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a company owned by the caller, stamped with the caller's Book."""
    company.created_at = datetime.now(timezone.utc)
    company.updated_at = datetime.now(timezone.utc)
    saved = await crud.create(db_session, user_id, company)

    # Create equity accounts in accounting service
    equity_accounts = [
        ("SHARE_CAPITAL", f"Share Capital - {company.company_name}", "Equity"),
        ("SHARE_PREMIUM", f"Share Premium Account - {company.company_name}", "Equity"),
        ("RETAINED_EARNINGS", f"Retained Earnings - {company.company_name}", "Equity"),
        ("PROFIT_LOSS_CURRENT", f"Profit & Loss - Current Year - {company.company_name}", "Equity"),
    ]

    for code, name, acc_type in equity_accounts:
        await call_accounting_service(
            "POST",
            "/accounts/",
            {
                "account_number": f"{company.company_code}-{code}",
                "account_name": name,
                "account_type": acc_type,
                "description": f"Equity account for {company.company_name}",
            },
        )

    # Log to audit
    await call_audit_service(
        {
            "event_type": "create",
            "resource_type": "company",
            "resource_id": saved.id,
            "user_id": user_id,
            "action_details": {"company_name": company.company_name, "type": company.company_type.value},
        }
    )

    return saved


@app.get("/companies")
async def list_companies(
    company_type: Optional[CompanyType] = None,
    status: Optional[CompanyStatus] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's Book-visible companies"""
    results = await crud.list_all(db_session, user_id, Company)

    if company_type:
        results = [c for c in results if c.company_type == company_type]
    if status:
        results = [c for c in results if c.status == status]

    return results


@app.get("/companies/{company_id}")
async def get_company(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a caller-owned, Book-visible company; cross-scope reads 404."""
    return await crud.find_or_404(db_session, user_id, Company, company_id)


@app.put("/companies/{company_id}")
async def update_company(
    company_id: str,
    company: Company,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update company details; cross-scope updates 404."""
    existing = await crud.find_or_404(db_session, user_id, Company, company_id)

    company.id = existing.id
    company.created_at = existing.created_at
    company.updated_at = datetime.now(timezone.utc)
    await crud.update(db_session, user_id, company)

    return company


# --- Shareholder Management ---


@app.post("/shareholders")
async def register_shareholder(
    shareholder: Shareholder,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Register a shareholder (caller-owned, Book-stamped)."""
    shareholder.created_at = datetime.now(timezone.utc)
    saved = await crud.create(db_session, user_id, shareholder)

    # Update percentage holdings for all the caller's shareholders in this company
    await _recalculate_shareholding_percentages(db_session, user_id, shareholder.company_id)

    # Return the recalculated record (the original returned the live dict entry)
    return await crud.find(db_session, user_id, Shareholder, saved.id)


@app.get("/shareholders")
async def list_shareholders(
    company_id: Optional[str] = None,
    share_class: Optional[ShareClass] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's Book-visible shareholders"""
    results = await crud.list_all(db_session, user_id, Shareholder)

    if company_id:
        results = [s for s in results if s.company_id == company_id]
    if share_class:
        results = [s for s in results if s.share_class == share_class]

    return results


async def _recalculate_shareholding_percentages(db_session: AsyncSession, user_id: str, company_id: str):
    """Recalculate percentage holdings across the caller's shareholders in a company."""
    company_shareholders = [
        s for s in await crud.list_all(db_session, user_id, Shareholder) if s.company_id == company_id
    ]
    total_shares = sum(s.shares_held for s in company_shareholders)

    for shareholder in company_shareholders:
        if total_shares > 0:
            shareholder.percentage_holding = (shareholder.shares_held / total_shares) * 100
            await crud.update(db_session, user_id, shareholder)


# --- Share Capital Management ---


@app.post("/share-capital")
async def create_share_capital(
    share_capital: ShareCapital,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a share capital record"""
    share_capital.created_at = datetime.now(timezone.utc)

    # Calculate totals
    share_capital.total_paid_up_capital = (
        Decimal(str(share_capital.issued_shares)) * share_capital.paid_up_value_per_share
    )

    return await crud.create(db_session, user_id, share_capital)


@app.get("/share-capital")
async def get_share_capital(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's most recent share capital record for a company"""
    results = [sc for sc in await crud.list_all(db_session, user_id, ShareCapital) if sc.company_id == company_id]
    results.sort(key=lambda x: x.created_at)
    return results[-1] if results else None


# ============================================================================
# Capital Transactions
# ============================================================================


@app.post("/capital-transactions")
async def record_capital_transaction(
    transaction: CapitalTransaction,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record a capital transaction (caller-owned, Book-stamped)."""
    transaction.created_at = datetime.now(timezone.utc)
    saved = await crud.create(db_session, user_id, transaction)

    # Create journal entry in accounting service
    company = await crud.find(db_session, user_id, Company, transaction.company_id)

    if company:
        journal_lines = []

        if transaction.transaction_type == CapitalTransactionType.SHARE_ISSUANCE:
            journal_lines = [
                {
                    "account_code": f"{company.company_code}-BANK",
                    "description": f"Share issuance - {transaction.number_of_shares} shares",
                    "debit": True,
                    "amount": str(transaction.total_amount),
                },
                {
                    "account_code": f"{company.company_code}-SHARE_CAPITAL",
                    "description": f"Share capital - nominal value",
                    "debit": False,
                    "amount": str(Decimal(str(transaction.number_of_shares)) * transaction.price_per_share),
                },
            ]
            if transaction.share_premium_amount:
                journal_lines.append(
                    {
                        "account_code": f"{company.company_code}-SHARE_PREMIUM",
                        "description": "Share premium",
                        "debit": False,
                        "amount": str(transaction.share_premium_amount),
                    }
                )

        elif transaction.transaction_type == CapitalTransactionType.DIVIDEND_PAYMENT:
            journal_lines = [
                {
                    "account_code": f"{company.company_code}-DIVIDEND",
                    "description": "Dividend payment",
                    "debit": True,
                    "amount": str(transaction.total_amount),
                },
                {
                    "account_code": f"{company.company_code}-BANK",
                    "description": "Cash paid for dividends",
                    "debit": False,
                    "amount": str(transaction.total_amount),
                },
            ]

        if journal_lines:
            journal_result = await call_accounting_service(
                "POST",
                "/journal-entries/",
                {
                    "description": transaction.reason or f"Capital transaction: {transaction.transaction_type.value}",
                    "reference": transaction.reference_number,
                    "date": transaction.transaction_date.isoformat(),
                    "lines": journal_lines,
                },
            )
            saved.journal_entry_id = journal_result.get("id")
            await crud.update(db_session, user_id, saved)

    return saved


@app.get("/capital-transactions")
async def list_capital_transactions(
    company_id: Optional[str] = None,
    transaction_type: Optional[CapitalTransactionType] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's Book-visible capital transactions"""
    results = await crud.list_all(db_session, user_id, CapitalTransaction)

    if company_id:
        results = [t for t in results if t.company_id == company_id]
    if transaction_type:
        results = [t for t in results if t.transaction_type == transaction_type]
    if start_date:
        results = [t for t in results if t.transaction_date >= start_date]
    if end_date:
        results = [t for t in results if t.transaction_date <= end_date]

    results.sort(key=lambda x: x.transaction_date, reverse=True)
    return results


# ============================================================================
# Dividend Management
# ============================================================================


@app.post("/dividends")
async def declare_dividend(
    dividend: Dividend,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Declare a dividend and generate per-shareholder payments for the caller's shareholders."""
    dividend.created_at = datetime.now(timezone.utc)
    dividend.net_payment = dividend.total_amount - dividend.tax_withheld

    saved = await crud.create(db_session, user_id, dividend)

    # Generate individual dividend payments for each of the caller's shareholders
    company_shareholders = [
        s for s in await crud.list_all(db_session, user_id, Shareholder) if s.company_id == dividend.company_id
    ]
    for shareholder in company_shareholders:
        if shareholder.share_class == dividend.share_class:
            shares = shareholder.shares_held
            gross = shares * dividend.per_share_amount
            tax = gross * Decimal("0.1")  # Assume 10% withholding
            net = gross - tax

            payment = DividendPayment(
                id=str(uuid.uuid4()),
                dividend_id=saved.id,
                shareholder_id=shareholder.id,
                shareholder_name=shareholder.shareholder_name,
                shares_held=shares,
                gross_amount=gross,
                tax_withheld=tax,
                net_amount=net,
                created_at=datetime.now(timezone.utc),
            )
            await crud.create(db_session, user_id, payment)

    # Create journal entry
    company = await crud.find(db_session, user_id, Company, dividend.company_id)
    if company:
        await call_accounting_service(
            "POST",
            "/journal-entries/",
            {
                "description": f"Dividend declared - {dividend.dividend_type.value}",
                "reference": f"DIV-{saved.id[:8]}",
                "date": dividend.declaration_date.isoformat(),
                "lines": [
                    {
                        "account_code": f"{company.company_code}-PROFIT_LOSS_CURRENT",
                        "description": "Proposed dividend",
                        "debit": True,
                        "amount": str(dividend.total_amount),
                    },
                    {
                        "account_code": f"{company.company_code}-DIVIDEND_PAYABLE",
                        "description": "Dividend payable",
                        "debit": False,
                        "amount": str(dividend.total_amount),
                    },
                ],
            },
        )

    return saved


@app.get("/dividends")
async def list_dividends(
    company_id: Optional[str] = None,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's Book-visible dividends"""
    results = await crud.list_all(db_session, user_id, Dividend)

    if company_id:
        results = [d for d in results if d.company_id == company_id]
    if status:
        results = [d for d in results if d.status == status]

    return results


@app.post("/dividends/{dividend_id}/pay")
async def pay_dividend(
    dividend_id: str,
    approved_by: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Process dividend payment; cross-scope payments 404."""
    dividend = await crud.find_or_404(db_session, user_id, Dividend, dividend_id)

    dividend.status = "paid"
    dividend.approved_by = approved_by
    await crud.update(db_session, user_id, dividend)

    # Update the caller's individual payments for this dividend
    for payment in await crud.list_all(db_session, user_id, DividendPayment):
        if payment.dividend_id == dividend_id:
            payment.status = "processed"
            payment.payment_date = datetime.now(timezone.utc)
            await crud.update(db_session, user_id, payment)

    # Create journal entry
    company = await crud.find(db_session, user_id, Company, dividend.company_id)
    if company:
        await call_accounting_service(
            "POST",
            "/journal-entries/",
            {
                "description": f"Dividend payment - {dividend.dividend_type.value}",
                "reference": f"DIV-PAY-{dividend.id[:8]}",
                "date": datetime.now(timezone.utc).isoformat(),
                "lines": [
                    {
                        "account_code": f"{company.company_code}-DIVIDEND_PAYABLE",
                        "description": "Clear dividend payable",
                        "debit": True,
                        "amount": str(dividend.total_amount),
                    },
                    {
                        "account_code": f"{company.company_code}-BANK",
                        "description": "Cash paid for dividends",
                        "debit": False,
                        "amount": str(dividend.net_payment),
                    },
                    {
                        "account_code": f"{company.company_code}-TAX",
                        "description": "Tax withheld on dividends",
                        "debit": False,
                        "amount": str(dividend.tax_withheld),
                    },
                ],
            },
        )

    return dividend


@app.get("/dividends/{dividend_id}/payments")
async def get_dividend_payments(
    dividend_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's individual payments for a Book-visible dividend."""
    await crud.find_or_404(db_session, user_id, Dividend, dividend_id)
    return [p for p in await crud.list_all(db_session, user_id, DividendPayment) if p.dividend_id == dividend_id]


# ============================================================================
# Retained Earnings
# ============================================================================


@app.post("/retained-earnings")
async def calculate_retained_earnings(
    entry: RetainedEarnings,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate and record retained earnings (caller-owned)."""
    entry.created_at = datetime.now(timezone.utc)

    # Calculate closing balance
    entry.closing_balance = (
        entry.opening_balance
        + entry.net_profit_for_period
        - entry.dividends_declared
        + entry.prior_year_adjustments
        - entry.transfers_to_reserves
    )

    return await crud.create(db_session, user_id, entry)


@app.get("/retained-earnings")
async def get_retained_earnings(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's retained earnings history for a company"""
    results = [e for e in await crud.list_all(db_session, user_id, RetainedEarnings) if e.company_id == company_id]
    results.sort(key=lambda x: x.created_at)
    return results


# ============================================================================
# Reserves
# ============================================================================


@app.post("/reserves")
async def create_reserve(
    reserve: Reserve,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a reserve record (caller-owned)."""
    reserve.created_at = datetime.now(timezone.utc)
    reserve.closing_balance = reserve.opening_balance + reserve.transfers_in - reserve.transfers_out

    return await crud.create(db_session, user_id, reserve)


@app.get("/reserves")
async def get_reserves(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's reserves for a company"""
    return [r for r in await crud.list_all(db_session, user_id, Reserve) if r.company_id == company_id]


# ============================================================================
# Reports
# ============================================================================


@app.get("/reports/equity-statement/{company_id}")
async def get_equity_statement(
    company_id: str,
    as_of_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Generate a statement of changes in equity from the caller's Book-visible records."""
    company = await crud.find_or_404(db_session, user_id, Company, company_id)

    # Get share capital
    share_capitals = [
        sc for sc in await crud.list_all(db_session, user_id, ShareCapital) if sc.company_id == company_id
    ]
    share_capitals.sort(key=lambda x: x.created_at)
    share_capital = share_capitals[-1] if share_capitals else None
    share_capital_amount = share_capital.total_paid_up_capital if share_capital else Decimal("0")
    share_premium_amount = share_capital.share_premium if share_capital else Decimal("0")

    # Get retained earnings
    re_entries = [e for e in await crud.list_all(db_session, user_id, RetainedEarnings) if e.company_id == company_id]
    re_entries.sort(key=lambda x: x.created_at)
    current_re = re_entries[-1].closing_balance if re_entries else Decimal("0")

    # Get reserves
    company_reserves = [r for r in await crud.list_all(db_session, user_id, Reserve) if r.company_id == company_id]
    total_reserves = sum(r.closing_balance for r in company_reserves)

    # Get movements for the period
    movements = []
    for transaction in await crud.list_all(db_session, user_id, CapitalTransaction):
        if transaction.company_id == company_id and transaction.transaction_date <= as_of_date:
            movements.append(
                {
                    "date": transaction.transaction_date.isoformat(),
                    "type": transaction.transaction_type.value,
                    "description": transaction.reason,
                    "amount": str(transaction.total_amount),
                }
            )

    total_equity = share_capital_amount + share_premium_amount + total_reserves + current_re

    return models.EquityReport(
        id=str(uuid.uuid4()),
        company_id=company_id,
        report_date=as_of_date,
        share_capital=share_capital_amount,
        share_premium=share_premium_amount,
        reserves=total_reserves,
        retained_earnings=current_re,
        total_equity=total_equity,
        movements=movements,
        generated_at=datetime.now(timezone.utc),
    )


@app.get("/reports/shareholder-register/{company_id}")
async def get_shareholder_register(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Generate a shareholder register from the caller's Book-visible records."""
    company = await crud.find_or_404(db_session, user_id, Company, company_id)

    company_shareholders = [
        s for s in await crud.list_all(db_session, user_id, Shareholder) if s.company_id == company_id
    ]

    # Sort by percentage holding
    company_shareholders.sort(key=lambda x: x.percentage_holding, reverse=True)

    return {
        "company": company,
        "total_shareholders": len(company_shareholders),
        "shareholders": [
            {
                "name": s.shareholder_name,
                "type": s.shareholder_type,
                "shares_held": s.shares_held,
                "percentage": f"{s.percentage_holding:.2f}%",
                "controlling_party": s.is_controlling_party,
            }
            for s in company_shareholders
        ],
    }


@app.get("/reports/dividend-history/{company_id}")
async def get_dividend_history(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's dividend payment history for a company"""
    company_dividends = [d for d in await crud.list_all(db_session, user_id, Dividend) if d.company_id == company_id]
    company_dividends.sort(key=lambda x: x.declaration_date, reverse=True)

    return {
        "company_id": company_id,
        "total_dividends_declared": len(company_dividends),
        "total_amount": str(sum(d.total_amount for d in company_dividends)),
        "dividends": company_dividends,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
