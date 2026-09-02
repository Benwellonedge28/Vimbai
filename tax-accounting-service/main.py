"""
Vimbai Tax Accounting Service
Tax calculation, compliance, reporting, and planning for multiple jurisdictions.
Supports VAT/GST, income tax, withholding tax, and indirect taxes.
"""

import os
import uuid
from datetime import datetime, timedelta
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

# ============================================================================
# Configuration
# ============================================================================

SERVICE_NAME = "tax-accounting-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8022"))

# Internal service URLs
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ADMIN_SERVICE_URL = os.getenv("ADMIN_SERVICE_URL", "http://localhost:8001")

# ============================================================================
# Logging Configuration
# ============================================================================

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

# ============================================================================
# FastAPI Application
# ============================================================================

app = FastAPI(
    title="Vimbai Tax Accounting Service",
    description="Tax calculation, compliance, reporting, and planning for multiple jurisdictions",
    version=SERVICE_VERSION,
    docs_url="/docs",
    redoc_url="/redoc",
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


# ============================================================================
# Enums
# ============================================================================


class TaxType(str, Enum):
    VAT = "vat"  # Value Added Tax
    GST = "gst"  # Goods and Services Tax
    SALES_TAX = "sales_tax"
    INCOME_TAX = "income_tax"
    CORPORATE_TAX = "corporate_tax"
    WITHHOLDING_TAX = "withholding_tax"
    PAYROLL_TAX = "payroll_tax"
    EXCISE_DUTY = "excise_duty"
    CUSTOMS_DUTY = "customs_duty"
    PROPERTY_TAX = "property_tax"
    CAPITAL_GAINS_TAX = "capital_gains_tax"
    DIVIDEND_TAX = "dividend_tax"
    ROYALTY_TAX = "royalty_tax"
    SERVICE_TAX = "service_tax"
    ENVIRONMENTAL_TAX = "environmental_tax"
    STAMP_DUTY = "stamp_duty"
    TRANSFER_TAX = "transfer_tax"


class TaxRegime(str, Enum):
    STANDARD = "standard"
    CASH_BASIS = "cash_basis"
    ACCRUAL_BASIS = "accrual_basis"
    COMPOSITE = "composite"
    FLAT_RATE = "flat_rate"
    SIMPLIFIED = "simplified"
    EXEMPT = "exempt"


class FilingFrequency(str, Enum):
    MONTHLY = "monthly"
    QUARTERLY = "quarterly"
    ANNUALLY = "annually"
    BIANNUAL = "biannual"


class TransactionType(str, Enum):
    SALE = "sale"
    PURCHASE = "purchase"
    EXPENSE = "expense"
    REVENUE = "revenue"
    IMPORT = "import"
    EXPORT = "export"
    INTERCOMPANY = "intercompany"
    DIVIDEND = "dividend"
    INTEREST = "interest"
    ROYALTY = "royalty"
    SERVICE = "service"


class JurisdictionType(str, Enum):
    FEDERAL = "federal"
    STATE = "state"
    PROVINCE = "province"
    COUNTY = "county"
    CITY = "city"
    MUNICIPAL = "municipal"
    REGIONAL = "regional"


class RateType(str, Enum):
    STANDARD = "standard"
    REDUCED = "reduced"
    ZERO = "zero"
    EXEMPT = "exempt"
    NEGATIVE = "negative"


# ============================================================================
# Pydantic Models
# ============================================================================


class TaxRate(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    tax_type: TaxType
    jurisdiction: str
    jurisdiction_type: JurisdictionType
    rate_type: RateType
    rate_percentage: float
    effective_from: datetime
    effective_to: Optional[datetime] = None
    description: Optional[str] = None
    is_active: bool = True


class TaxRegistration(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    tax_type: TaxType
    registration_number: str
    jurisdiction: str
    registration_date: datetime
    effective_date: datetime
    cancellation_date: Optional[datetime] = None
    is_active: bool = True
    filing_frequency: FilingFrequency = FilingFrequency.MONTHLY
    threshold_amount: Optional[float] = None
    notes: Optional[str] = None


class TaxConfig(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    tax_type: TaxType
    regime: TaxRegime = TaxRegime.STANDARD
    default_rate: float
    reduced_rate: Optional[float] = None
    zero_rated_categories: List[str] = []
    exempt_categories: List[str] = []
    reverse_charge_enabled: bool = False
    is_active: bool = True


class TaxTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    transaction_date: datetime
    transaction_type: TransactionType
    tax_type: TaxType
    jurisdiction: str
    gross_amount: float
    net_amount: float
    tax_amount: float
    rate_used: float
    rate_type: RateType
    is_reversed: bool = False
    reverse_journal_entry_id: Optional[str] = None
    reference: Optional[str] = None
    description: Optional[str] = None
    linked_transaction_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class TaxReturn(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    tax_type: TaxType
    jurisdiction: str
    period_start: datetime
    period_end: datetime
    filing_frequency: FilingFrequency
    gross_sales: float = 0
    gross_purchases: float = 0
    tax_collected: float = 0
    tax_paid: float = 0
    input_tax_credit: float = 0
    tax_adjustments: float = 0
    net_tax_due: float = 0
    status: str = "draft"  # draft, filed, paid, overdue
    due_date: Optional[datetime] = None
    filed_date: Optional[datetime] = None
    paid_date: Optional[datetime] = None
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class WithholdingTaxEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    payment_date: datetime
    recipient_id: str
    recipient_name: str
    recipient_country: str
    payment_type: TransactionType
    gross_amount: float
    withholding_rate: float
    withholding_amount: float
    net_amount_paid: float
    tax_certificate_number: Optional[str] = None
    is_reconciled: bool = False
    created_at: datetime = Field(default_factory=datetime.utcnow)


class DeferredTax(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    period_end: datetime
    deferred_tax_asset: float = 0
    deferred_tax_liability: float = 0
    tax_rate: float
    temporary_differences: List[Dict[str, Any]] = []
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class TaxReport(BaseModel):
    report_id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    report_type: str
    tax_type: TaxType
    jurisdiction: str
    period_start: datetime
    period_end: datetime
    generated_at: datetime = Field(default_factory=datetime.utcnow)
    summary: Dict[str, Any] = {}
    details: List[Dict[str, Any]] = []
    total_tax: float = 0


class TaxLiabilitySchedule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    tax_type: TaxType
    jurisdiction: str
    period: str
    total_liability: float = 0
    paid_amount: float = 0
    outstanding_amount: float = 0
    due_date: datetime
    status: str = "pending"
    entries: List[Dict[str, Any]] = []


# ============================================================================
# In-Memory Storage
# ============================================================================

tax_rates: Dict[str, TaxRate] = {}
tax_registrations: Dict[str, TaxRegistration] = {}
tax_configs: Dict[str, TaxConfig] = {}
tax_transactions: List[TaxTransaction] = []
tax_returns: List[TaxReturn] = []
withholding_entries: List[WithholdingTaxEntry] = []
deferred_taxes: List[DeferredTax] = []
tax_reports: List[TaxReport] = []


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


def get_applicable_rate(tax_type: TaxType, jurisdiction: str, transaction_type: TransactionType) -> Optional[TaxRate]:
    """Get the applicable tax rate for a transaction."""
    for rate in tax_rates.values():
        if rate.tax_type == tax_type and rate.jurisdiction == jurisdiction and rate.is_active:
            if rate.effective_from <= datetime.utcnow():
                if rate.effective_to is None or rate.effective_to >= datetime.utcnow():
                    return rate
    return None


def calculate_vat_gst(amount: float, rate: float) -> tuple[float, float]:
    """Calculate VAT/GST from gross or net amount."""
    tax_amount = amount * (rate / 100)
    net_amount = amount - tax_amount if amount > 0 else 0
    return net_amount, tax_amount


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


@app.post("/tax-rates", response_model=TaxRate, status_code=status.HTTP_201_CREATED)
async def create_tax_rate(data: TaxRate):
    """Create a new tax rate."""
    rate_id = str(uuid.uuid4())
    data.id = rate_id
    tax_rates[rate_id] = data

    await call_audit_service(
        "CREATE", "tax_rate", rate_id, {"rate": data.rate_percentage, "jurisdiction": data.jurisdiction}
    )
    logger.info("tax_rate_created", rate_id=rate_id, rate=data.rate_percentage)
    return data


@app.get("/tax-rates")
async def list_tax_rates(
    tax_type: Optional[TaxType] = None, jurisdiction: Optional[str] = None, is_active: Optional[bool] = None
):
    """List tax rates with filters."""
    result = list(tax_rates.values())

    if tax_type:
        result = [r for r in result if r.tax_type == tax_type]
    if jurisdiction:
        result = [r for r in result if r.jurisdiction == jurisdiction]
    if is_active is not None:
        result = [r for r in result if r.is_active == is_active]

    return {"rates": result, "count": len(result)}


@app.get("/tax-rates/{rate_id}")
async def get_tax_rate(rate_id: str):
    """Get tax rate details."""
    rate = tax_rates.get(rate_id)
    if not rate:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Tax rate {rate_id} not found")
    return rate


@app.put("/tax-rates/{rate_id}")
async def update_tax_rate(rate_id: str, data: Dict[str, Any]):
    """Update tax rate."""
    rate = tax_rates.get(rate_id)
    if not rate:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Tax rate {rate_id} not found")

    for key, value in data.items():
        if hasattr(rate, key) and key not in ["id"]:
            setattr(rate, key, value)

    await call_audit_service("UPDATE", "tax_rate", rate_id, {"updated_fields": list(data.keys())})
    return rate


# ============================================================================
# API Endpoints - Tax Registrations
# ============================================================================


@app.post("/tax-registrations", response_model=TaxRegistration, status_code=status.HTTP_201_CREATED)
async def create_tax_registration(data: TaxRegistration):
    """Register for a tax type in a jurisdiction."""
    reg_id = str(uuid.uuid4())
    data.id = reg_id
    tax_registrations[reg_id] = data

    await call_audit_service(
        "CREATE", "tax_registration", reg_id, {"tax_type": data.tax_type, "jurisdiction": data.jurisdiction}
    )
    return data


@app.get("/tax-registrations")
async def list_tax_registrations(
    tax_type: Optional[TaxType] = None, jurisdiction: Optional[str] = None, is_active: Optional[bool] = None
):
    """List tax registrations."""
    result = list(tax_registrations.values())

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


@app.post("/calculate", response_model=TaxTransaction)
async def calculate_tax(
    transaction_date: datetime,
    transaction_type: TransactionType,
    tax_type: TaxType,
    jurisdiction: str,
    gross_amount: float,
    is_net: bool = False,
):
    """Calculate tax on a transaction."""
    rate = get_applicable_rate(tax_type, jurisdiction, transaction_type)

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

    transaction = TaxTransaction(
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

    tax_transactions.append(transaction)

    # Create journal entry for tax
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


@app.post("/calculate-batch", response_model=List[TaxTransaction])
async def calculate_tax_batch(transactions: List[Dict[str, Any]]):
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
        )
        results.append(result)
    return results


# ============================================================================
# API Endpoints - Tax Returns
# ============================================================================


@app.post("/tax-returns", response_model=TaxReturn, status_code=status.HTTP_201_CREATED)
async def create_tax_return(data: TaxReturn):
    """Create a tax return for a period."""
    tax_return_id = str(uuid.uuid4())
    data.id = tax_return_id
    data.created_at = datetime.utcnow()

    # Calculate from transactions
    period_transactions = [
        t
        for t in tax_transactions
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

    tax_returns.append(data)

    await call_audit_service(
        "CREATE",
        "tax_return",
        tax_return_id,
        {
            "tax_type": data.tax_type,
            "period": f"{data.period_start} to {data.period_end}",
            "net_tax_due": data.net_tax_due,
        },
    )

    return data


@app.get("/tax-returns")
async def list_tax_returns(
    tax_type: Optional[TaxType] = None,
    jurisdiction: Optional[str] = None,
    status: Optional[str] = None,
    period_start: Optional[datetime] = None,
):
    """List tax returns."""
    result = list(tax_returns)

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
async def file_tax_return(return_id: str):
    """Mark a tax return as filed."""
    tax_return = next((r for r in tax_returns if r.id == return_id), None)
    if not tax_return:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Tax return {return_id} not found")

    tax_return.status = "filed"
    tax_return.filed_date = datetime.utcnow()

    await call_audit_service("FILE", "tax_return", return_id, {"filed_date": tax_return.filed_date.isoformat()})
    return tax_return


@app.post("/tax-returns/{return_id}/pay")
async def pay_tax_return(return_id: str):
    """Record payment of tax due."""
    tax_return = next((r for r in tax_returns if r.id == return_id), None)
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

    await call_audit_service("PAY", "tax_return", return_id, {"amount": tax_return.net_tax_due})
    return tax_return


# ============================================================================
# API Endpoints - Withholding Tax
# ============================================================================


@app.post("/withholding-tax", response_model=WithholdingTaxEntry, status_code=status.HTTP_201_CREATED)
async def record_withholding_tax(data: WithholdingTaxEntry):
    """Record withholding tax on payments."""
    entry_id = str(uuid.uuid4())
    data.id = entry_id
    data.created_at = datetime.utcnow()

    withholding_entries.append(data)

    # Create journal entry
    journal_entry = {
        "date": data.payment_date,
        "description": f"Withholding tax on payment to {data.recipient_name}",
        "entries": [
            {
                "account_code": "5100",
                "description": "Expense/Service",
                "debit": data.gross_amount - data.withholding_amount,
                "credit": 0,
            },
            {
                "account_code": "2200",
                "description": "Withholding Tax Payable",
                "debit": 0,
                "credit": data.withholding_amount,
            },
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": data.net_amount_paid},
        ],
        "reference": f"WHT-{entry_id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)

    await call_audit_service("CREATE", "withholding_tax", entry_id, {"amount": data.withholding_amount})
    return data


@app.get("/withholding-tax")
async def list_withholding_entries(
    recipient_id: Optional[str] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    is_reconciled: Optional[bool] = None,
):
    """List withholding tax entries."""
    result = list(withholding_entries)

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


@app.post("/reports/tax-summary", response_model=TaxReport)
async def generate_tax_summary_report(
    tax_type: TaxType,
    jurisdiction: str,
    period_start: datetime,
    period_end: datetime,
):
    """Generate a tax summary report."""
    transactions = [
        t
        for t in tax_transactions
        if t.tax_type == tax_type
        and t.jurisdiction == jurisdiction
        and period_start <= t.transaction_date <= period_end
        and not t.is_reversed
    ]

    sales = [t for t in transactions if t.transaction_type == TransactionType.SALE]
    purchases = [t for t in transactions if t.transaction_type == TransactionType.PURCHASE]

    report = TaxReport(
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

    tax_reports.append(report)
    return report


@app.get("/reports/vat-by-jurisdiction")
async def get_vat_by_jurisdiction(period_start: datetime, period_end: datetime):
    """Get VAT breakdown by jurisdiction."""
    transactions = [
        t
        for t in tax_transactions
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


@app.post("/deferred-tax", response_model=DeferredTax, status_code=status.HTTP_201_CREATED)
async def calculate_deferred_tax(period_end: datetime, tax_rate: float, temporary_differences: List[Dict[str, Any]]):
    """Calculate deferred tax assets and liabilities."""
    deferred_tax = DeferredTax(
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

    # Create journal entry
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

    deferred_taxes.append(deferred_tax)
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
):
    """Get tax liability schedule."""
    returns = [r for r in tax_returns if r.status in ["draft", "filed"]]

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

    logger.info("starting_tax_accounting_service", port=PORT)
    uvicorn.run(app, host="0.0.0.0", port=PORT)
