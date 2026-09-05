# This file may be imported bare (Docker `uvicorn main:app`, bracket mounts), so it
# bootstraps its own package alias before importing sibling modules.
import importlib.util as _ilu
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
_PKG = "tax_accounting_service"
if _PKG not in _sys.modules or not hasattr(_sys.modules.get(_PKG), "__path__"):
    _spec = _ilu.spec_from_file_location(_PKG, _os.path.join(_HERE, "__init__.py"))
    _pkg = _ilu.module_from_spec(_spec)
    _pkg.__path__ = [_HERE]
    _sys.modules[_PKG] = _pkg

"""
Vimbai Tax Accounting Service
Tax calculation, compliance, reporting, and planning.

Record-keeping only: this service records tax positions and journal-entry
references (user-owned, Book-scoped via X-User-Id / X-Book-ID); it never
moves money. Corrections use reversing entries.
"""

import os
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, HTTPException, Request, status
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from neo4j import AsyncSession
from tax_accounting_service import crud, models
from tax_accounting_service.database import Neo4jConnector
from tax_accounting_service.dependencies import book_id_var, get_db_session, get_user_id
from tax_accounting_service.exceptions import (
    ConflictError,
    NotFoundError,
    TaxAccountingError,
    ValidationError,
)
from tax_accounting_service.models import (
    DeferredTax,
    FilingFrequency,
    RateType,
    TaxRate,
    TaxRegime,
    TaxRegistration,
    TaxReport,
    TaxReturn,
    TaxTransaction,
    TaxType,
    TransactionType,
    WithholdingTaxEntry,
)

SERVICE_NAME = "tax-accounting-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8060"))
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

app = FastAPI(title="Vimbai Tax Accounting Service", version=SERVICE_VERSION)
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


# ============================================================================
# Internal Service Communication
# ============================================================================


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    """Call the main accounting service."""
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "GET":
                response = await client.get(url)
            elif method == "POST":
                response = await client.post(url, json=data)
            else:
                raise ValueError(f"Unsupported HTTP method: {method}")

            if response.status_code in [200, 201]:
                return response.json() if response.text else {}
            return {}
    except Exception as e:
        logger.error("accounting_service_call_error", error=str(e))
        return {}


async def call_audit_service(action: str, resource_type: str, resource_id: str, details: Dict[str, Any]) -> None:
    """Log actions to the audit service."""
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
    except Exception as e:
        logger.error("audit_service_call_error", error=str(e))


# ============================================================================
# Helper Functions
# ============================================================================


def get_applicable_rate(rates: List[TaxRate], tax_type: TaxType, jurisdiction: str) -> Optional[TaxRate]:
    """Get the applicable tax rate for a transaction."""
    now = datetime.now(timezone.utc)
    for rate in rates:
        if rate.tax_type == tax_type and rate.jurisdiction == jurisdiction and rate.is_active:
            if rate.effective_from <= now:
                if rate.effective_to is None or rate.effective_to >= now:
                    return rate
    return None


def get_default_rates() -> Dict[str, float]:
    """Get default tax rates by type."""
    return {
        "vat_standard": 20.0,
        "vat_reduced": 10.0,
        "vat_zero": 0.0,
        "gst_standard": 10.0,
        "sales_tax_standard": 8.0,
        "withholding_standard": 15.0,
        "corporate_income": 25.0,
        "capital_gains": 20.0,
    }


# ============================================================================
# API Endpoints - Health & Info
# ============================================================================


@app.get("/health")
async def health_check():
    """Health check endpoint."""
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "status": "healthy",
        "timestamp": datetime.utcnow().isoformat(),
    }


@app.get("/")
async def root():
    """Root endpoint."""
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "description": "Tax calculation, compliance, reporting, and planning",
    }


# ============================================================================
# API Endpoints - Tax Rates
# ============================================================================


@app.post("/tax-rates", response_model=models.TaxRate, status_code=status.HTTP_201_CREATED)
async def create_tax_rate(
    data: models.TaxRate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a new tax rate."""
    rate = await crud.create_tax_rate(db_session, user_id, data)

    await call_audit_service(
        "CREATE", "tax_rate", rate.id, {"rate": rate.rate_percentage, "jurisdiction": rate.jurisdiction}
    )
    logger.info("tax_rate_created", rate_id=rate.id, rate=rate.rate_percentage)
    return rate


@app.get("/tax-rates")
async def list_tax_rates(
    tax_type: Optional[TaxType] = None,
    jurisdiction: Optional[str] = None,
    is_active: Optional[bool] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List tax rates with filters."""
    result = await crud.list_tax_rates(db_session, user_id)

    if tax_type:
        result = [r for r in result if r.tax_type == tax_type]
    if jurisdiction:
        result = [r for r in result if r.jurisdiction == jurisdiction]
    if is_active is not None:
        result = [r for r in result if r.is_active == is_active]

    return {"rates": result, "count": len(result)}


@app.get("/tax-rates/{rate_id}")
async def get_tax_rate(
    rate_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get tax rate details."""
    rate = await crud.get_tax_rate(db_session, user_id, rate_id)
    if not rate:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Tax rate {rate_id} not found")
    return rate


@app.put("/tax-rates/{rate_id}")
async def update_tax_rate(
    rate_id: str,
    data: Dict[str, Any],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update tax rate."""
    rate = await crud.get_tax_rate(db_session, user_id, rate_id)
    if not rate:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Tax rate {rate_id} not found")

    for key, value in data.items():
        if hasattr(rate, key) and key not in ["id"]:
            setattr(rate, key, value)

    await crud.save_tax_rate(db_session, user_id, rate)

    await call_audit_service("UPDATE", "tax_rate", rate_id, {"updated_fields": list(data.keys())})
    return rate


# ============================================================================
# API Endpoints - Tax Registrations
# ============================================================================


@app.post("/tax-registrations", response_model=models.TaxRegistration, status_code=status.HTTP_201_CREATED)
async def create_tax_registration(
    data: models.TaxRegistration,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Register for a tax type in a jurisdiction."""
    registration = await crud.create_tax_registration(db_session, user_id, data)

    await call_audit_service(
        "CREATE",
        "tax_registration",
        registration.id,
        {"tax_type": registration.tax_type, "jurisdiction": registration.jurisdiction},
    )
    return registration


@app.get("/tax-registrations")
async def list_tax_registrations(
    tax_type: Optional[TaxType] = None,
    jurisdiction: Optional[str] = None,
    is_active: Optional[bool] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List tax registrations."""
    result = await crud.list_tax_registrations(db_session, user_id)

    if tax_type:
        result = [r for r in result if r.tax_type == tax_type]
    if jurisdiction:
        result = [r for r in result if r.jurisdiction == jurisdiction]
    if is_active is not None:
        result = [r for r in result if r.is_active == is_active]

    return {"registrations": result, "count": len(result)}


# ============================================================================
# API Endpoints - Tax Calculation
# ============================================================================


@app.post("/calculate", response_model=models.TaxTransaction)
async def calculate_tax(
    transaction_date: datetime,
    transaction_type: TransactionType,
    tax_type: TaxType,
    jurisdiction: str,
    gross_amount: float,
    is_net: bool = False,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate tax on a transaction."""
    rates = await crud.list_tax_rates(db_session, user_id)
    rate = get_applicable_rate(rates, tax_type, jurisdiction)

    if not rate:
        defaults = get_default_rates()
        rate_percentage = defaults.get(f"{tax_type.value}_standard", 20.0)
        rate_type = RateType.STANDARD
    else:
        rate_percentage = rate.rate_percentage
        rate_type = rate.rate_type

    if is_net:
        net_amount = gross_amount
        tax_amount = gross_amount * (rate_percentage / 100)
        gross_total = gross_amount + tax_amount
    else:
        gross_total = gross_amount
        tax_amount = gross_amount * (rate_percentage / 100) / (1 + rate_percentage / 100)
        net_amount = gross_amount - tax_amount

    transaction = models.TaxTransaction(
        transaction_date=transaction_date,
        transaction_type=transaction_type,
        tax_type=tax_type,
        jurisdiction=jurisdiction,
        gross_amount=gross_total,
        net_amount=net_amount,
        tax_amount=tax_amount,
        rate_used=rate_percentage,
        rate_type=rate_type,
        description=f"Tax calculation for {transaction_type.value} in {jurisdiction}",
    )

    transaction = await crud.create_tax_transaction(db_session, user_id, transaction)

    # Create journal entry for tax (record only — posted to accounting service)
    if transaction_type == TransactionType.SALE:
        journal_entry = {
            "date": transaction_date,
            "description": f"VAT collected on sale: {jurisdiction}",
            "entries": [
                {"account_code": "1100", "description": "Accounts Receivable", "debit": gross_total, "credit": 0},
                {"account_code": "4000", "description": "Sales Revenue", "debit": 0, "credit": net_amount},
                {
                    "account_code": "2200",
                    "description": f"VAT Payable - {jurisdiction}",
                    "debit": 0,
                    "credit": tax_amount,
                },
            ],
            "reference": f"TAX-{transaction.id[:8]}",
        }
    else:
        journal_entry = {
            "date": transaction_date,
            "description": f"VAT recoverable on purchase: {jurisdiction}",
            "entries": [
                {"account_code": "1500", "description": "Purchases", "debit": net_amount, "credit": 0},
                {
                    "account_code": "1300",
                    "description": f"VAT Receivable - {jurisdiction}",
                    "debit": tax_amount,
                    "credit": 0,
                },
                {"account_code": "2100", "description": "Accounts Payable", "debit": 0, "credit": gross_total},
            ],
            "reference": f"TAX-{transaction.id[:8]}",
        }

    await call_accounting_service("POST", "/journal-entries", journal_entry)

    await call_audit_service(
        "CALCULATE",
        "tax_transaction",
        transaction.id,
        {"tax_type": tax_type, "amount": tax_amount, "jurisdiction": jurisdiction},
    )

    return transaction


@app.post("/calculate-batch", response_model=List[models.TaxTransaction])
async def calculate_tax_batch(
    transactions: List[Dict[str, Any]],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate tax on multiple transactions."""
    results = []
    for txn in transactions:
        result = await calculate_tax(
            transaction_date=datetime.fromisoformat(txn.get("transaction_date", datetime.utcnow().isoformat())),
            transaction_type=TransactionType(txn["transaction_type"]),
            tax_type=TaxType(txn["tax_type"]),
            jurisdiction=txn["jurisdiction"],
            gross_amount=txn["gross_amount"],
            is_net=txn.get("is_net", False),
            user_id=user_id,
            db_session=db_session,
        )
        results.append(result)
    return results


# ============================================================================
# API Endpoints - Tax Returns
# ============================================================================


@app.post("/tax-returns", response_model=models.TaxReturn, status_code=status.HTTP_201_CREATED)
async def create_tax_return(
    data: models.TaxReturn,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a tax return for a period."""
    all_transactions = await crud.list_tax_transactions(db_session, user_id)

    # Calculate from transactions
    period_transactions = [
        t
        for t in all_transactions
        if t.tax_type == data.tax_type
        and t.jurisdiction == data.jurisdiction
        and data.period_start <= t.transaction_date <= data.period_end
        and not t.is_reversed
    ]

    sales_txns = [t for t in period_transactions if t.transaction_type == TransactionType.SALE]
    purchase_txns = [t for t in period_transactions if t.transaction_type == TransactionType.PURCHASE]

    data.gross_sales = sum(t.gross_amount for t in sales_txns)
    data.gross_purchases = sum(t.gross_amount for t in purchase_txns)
    data.tax_collected = sum(t.tax_amount for t in sales_txns)
    data.tax_paid = sum(t.tax_amount for t in purchase_txns)

    # Net tax due
    data.net_tax_due = data.tax_collected - data.tax_paid + data.tax_adjustments

    tax_return = await crud.create_tax_return(db_session, user_id, data)

    await call_audit_service(
        "CREATE",
        "tax_return",
        tax_return.id,
        {
            "tax_type": data.tax_type,
            "period": f"{data.period_start} to {data.period_end}",
            "net_tax_due": data.net_tax_due,
        },
    )

    return tax_return


@app.get("/tax-returns")
async def list_tax_returns(
    tax_type: Optional[TaxType] = None,
    jurisdiction: Optional[str] = None,
    status: Optional[str] = None,
    period_start: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List tax returns."""
    result = await crud.list_tax_returns(db_session, user_id)

    if tax_type:
        result = [r for r in result if r.tax_type == tax_type]
    if jurisdiction:
        result = [r for r in result if r.jurisdiction == jurisdiction]
    if status:
        result = [r for r in result if r.status == status]
    if period_start:
        result = [r for r in result if r.period_start >= period_start]

    return {"returns": result, "count": len(result)}


@app.post("/tax-returns/{return_id}/file")
async def file_tax_return(
    return_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Mark a tax return as filed."""
    tax_return = await crud.get_tax_return(db_session, user_id, return_id)
    if not tax_return:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Tax return {return_id} not found")

    tax_return.status = "filed"
    tax_return.filed_date = datetime.utcnow()
    await crud.save_tax_return(db_session, user_id, tax_return)

    await call_audit_service("FILE", "tax_return", return_id, {"status": "filed"})
    return tax_return


@app.post("/tax-returns/{return_id}/pay")
async def pay_tax_return(
    return_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record payment of tax due (records the payment + journal entry; moves no money)."""
    tax_return = await crud.get_tax_return(db_session, user_id, return_id)
    if not tax_return:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Tax return {return_id} not found")

    if tax_return.net_tax_due > 0:
        journal_entry = {
            "date": datetime.utcnow(),
            "description": f"Tax payment: {tax_return.tax_type.value} - {tax_return.jurisdiction}",
            "entries": [
                {
                    "account_code": "2200",
                    "description": "VAT/Tax Payable",
                    "debit": tax_return.net_tax_due,
                    "credit": 0,
                },
                {"account_code": "1000", "description": "Bank/Cash", "debit": 0, "credit": tax_return.net_tax_due},
            ],
            "reference": f"TAX-PAY-{return_id[:8]}",
        }
        result = await call_accounting_service("POST", "/journal-entries", journal_entry)
        tax_return.journal_entry_id = result.get("id")

    tax_return.status = "paid"
    tax_return.paid_date = datetime.utcnow()
    await crud.save_tax_return(db_session, user_id, tax_return)

    await call_audit_service("PAY", "tax_return", return_id, {"amount": tax_return.net_tax_due})
    return tax_return


# ============================================================================
# API Endpoints - Withholding Tax
# ============================================================================


@app.post("/withholding-tax", response_model=models.WithholdingTaxEntry, status_code=status.HTTP_201_CREATED)
async def record_withholding_tax(
    data: models.WithholdingTaxEntry,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record withholding tax on payments."""
    entry = await crud.create_withholding_entry(db_session, user_id, data)

    # Create journal entry
    journal_entry = {
        "date": entry.payment_date,
        "description": f"Withholding tax on payment to {entry.recipient_name}",
        "entries": [
            {
                "account_code": "5100",
                "description": "Expense/Service",
                "debit": entry.gross_amount - entry.withholding_amount,
                "credit": 0,
            },
            {
                "account_code": "2200",
                "description": "Withholding Tax Payable",
                "debit": 0,
                "credit": entry.withholding_amount,
            },
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": entry.net_amount_paid},
        ],
        "reference": f"WHT-{entry.id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)

    await call_audit_service("CREATE", "withholding_tax", entry.id, {"amount": entry.withholding_amount})
    return entry


@app.get("/withholding-tax")
async def list_withholding_entries(
    recipient_id: Optional[str] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    is_reconciled: Optional[bool] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List withholding tax entries."""
    result = await crud.list_withholding_entries(db_session, user_id)

    if recipient_id:
        result = [e for e in result if e.recipient_id == recipient_id]
    if start_date:
        result = [e for e in result if e.payment_date >= start_date]
    if end_date:
        result = [e for e in result if e.payment_date <= end_date]
    if is_reconciled is not None:
        result = [e for e in result if e.is_reconciled == is_reconciled]

    total_withheld = sum(e.withholding_amount for e in result)
    return {"entries": result, "total_withheld": total_withheld, "count": len(result)}


# ============================================================================
# API Endpoints - Tax Reports
# ============================================================================


@app.post("/reports/tax-summary", response_model=models.TaxReport)
async def generate_tax_summary_report(
    tax_type: TaxType,
    jurisdiction: str,
    period_start: datetime,
    period_end: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Generate a tax summary report."""
    all_transactions = await crud.list_tax_transactions(db_session, user_id)
    transactions = [
        t
        for t in all_transactions
        if t.tax_type == tax_type
        and t.jurisdiction == jurisdiction
        and period_start <= t.transaction_date <= period_end
        and not t.is_reversed
    ]

    sales = [t for t in transactions if t.transaction_type == TransactionType.SALE]
    purchases = [t for t in transactions if t.transaction_type == TransactionType.PURCHASE]

    report = models.TaxReport(
        report_type="tax_summary",
        tax_type=tax_type,
        jurisdiction=jurisdiction,
        period_start=period_start,
        period_end=period_end,
        summary={
            "total_transactions": len(transactions),
            "total_sales": len(sales),
            "total_purchases": len(purchases),
            "gross_sales": sum(t.gross_amount for t in sales),
            "gross_purchases": sum(t.gross_amount for t in purchases),
            "tax_collected": sum(t.tax_amount for t in sales),
            "tax_paid": sum(t.tax_amount for t in purchases),
            "net_tax_position": sum(t.tax_amount for t in sales) - sum(t.tax_amount for t in purchases),
        },
        details=[
            {
                "transaction_id": t.id,
                "date": t.transaction_date.isoformat(),
                "type": t.transaction_type.value,
                "gross": t.gross_amount,
                "tax": t.tax_amount,
            }
            for t in transactions
        ],
        total_tax=sum(t.tax_amount for t in transactions),
    )

    return await crud.create_tax_report(db_session, user_id, report)


@app.get("/reports/vat-by-jurisdiction")
async def get_vat_by_jurisdiction(
    period_start: datetime,
    period_end: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get VAT breakdown by jurisdiction."""
    all_transactions = await crud.list_tax_transactions(db_session, user_id)
    transactions = [
        t
        for t in all_transactions
        if t.tax_type in [TaxType.VAT, TaxType.GST]
        and period_start <= t.transaction_date <= period_end
        and not t.is_reversed
    ]

    by_jurisdiction = {}
    for txn in transactions:
        if txn.jurisdiction not in by_jurisdiction:
            by_jurisdiction[txn.jurisdiction] = {"collected": 0, "paid": 0, "count": 0}
        if txn.transaction_type == TransactionType.SALE:
            by_jurisdiction[txn.jurisdiction]["collected"] += txn.tax_amount
        else:
            by_jurisdiction[txn.jurisdiction]["paid"] += txn.tax_amount
        by_jurisdiction[txn.jurisdiction]["count"] += 1

    return {
        "period": {"start": period_start, "end": period_end},
        "by_jurisdiction": by_jurisdiction,
    }


# ============================================================================
# API Endpoints - Deferred Tax
# ============================================================================


@app.post("/deferred-tax", response_model=models.DeferredTax, status_code=status.HTTP_201_CREATED)
async def calculate_deferred_tax(
    period_end: datetime,
    tax_rate: float,
    temporary_differences: List[Dict[str, Any]],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate deferred tax assets and liabilities."""
    deferred_tax = models.DeferredTax(
        period_end=period_end,
        tax_rate=tax_rate,
        temporary_differences=temporary_differences,
    )

    for diff in temporary_differences:
        amount = diff.get("amount", 0)
        if diff.get("type") == "asset":
            deferred_tax.deferred_tax_asset += amount * (tax_rate / 100)
        else:
            deferred_tax.deferred_tax_liability += amount * (tax_rate / 100)

    # Create journal entry (record only)
    journal_entry = {
        "date": period_end,
        "description": "Deferred tax provision",
        "entries": [
            {
                "account_code": "1400",
                "description": "Deferred Tax Asset",
                "debit": deferred_tax.deferred_tax_asset,
                "credit": 0,
            },
            {
                "account_code": "2500",
                "description": "Deferred Tax Liability",
                "debit": 0,
                "credit": deferred_tax.deferred_tax_liability,
            },
            {
                "account_code": "8000",
                "description": "Tax Expense",
                "debit": 0,
                "credit": deferred_tax.deferred_tax_liability,
            },
        ],
        "reference": f"DT-{deferred_tax.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    deferred_tax.journal_entry_id = result.get("id")

    deferred_tax = await crud.create_deferred_tax(db_session, user_id, deferred_tax)
    await call_audit_service(
        "CALCULATE",
        "deferred_tax",
        deferred_tax.id,
        {"asset": deferred_tax.deferred_tax_asset, "liability": deferred_tax.deferred_tax_liability},
    )

    return deferred_tax


# ============================================================================
# API Endpoints - Tax Liability Schedule
# ============================================================================


@app.get("/liability-schedule")
async def get_tax_liability_schedule(
    tax_type: Optional[TaxType] = None,
    jurisdiction: Optional[str] = None,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get tax liability schedule."""
    all_returns = await crud.list_tax_returns(db_session, user_id)
    returns = [r for r in all_returns if r.status in ["draft", "filed"]]

    if tax_type:
        returns = [r for r in returns if r.tax_type == tax_type]
    if jurisdiction:
        returns = [r for r in returns if r.jurisdiction == jurisdiction]
    if status:
        returns = [r for r in returns if r.status == status]

    total_outstanding = sum(r.net_tax_due for r in returns if r.status == "filed")

    return {
        "returns": returns,
        "total_outstanding": total_outstanding,
        "count": len(returns),
    }


# ============================================================================
# Main Entry Point
# ============================================================================


if __name__ == "__main__":
    import uvicorn

    logger.info("starting_service", service=SERVICE_NAME, port=PORT)
    uvicorn.run(app, host="0.0.0.0", port=PORT)
