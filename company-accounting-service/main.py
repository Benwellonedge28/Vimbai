"""
Vimbai Company Accounting Service
Dedicated service for company-level financial management including
shareholder equity, dividends, capital transactions, and company-specific reporting
Uses existing services via internal API calls
"""

import os
import uuid
from datetime import datetime, timezone
from decimal import Decimal
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
from fastapi import Depends, FastAPI, HTTPException, Query
from pydantic import BaseModel, Field

app = FastAPI(
    title="Vimbai Company Accounting Service",
    description="Company-level financial management including shareholder equity, dividends, capital transactions, and company-specific reporting",
    version="1.0.0",
)

# ============================================================================
# Configuration - Internal API endpoints
# ============================================================================

ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8091")
CURRENCY_SERVICE_URL = os.getenv("CURRENCY_SERVICE_URL", "http://localhost:8020")
REPORTING_SERVICE_URL = os.getenv("REPORTING_SERVICE_URL", "http://localhost:8003")

# ============================================================================
# Enums
# ============================================================================


class CompanyType(str, Enum):
    SOLE_PROPRIETORSHIP = "sole_proprietorship"
    PARTNERSHIP = "partnership"
    LIMITED_COMPANY = "limited_company"
    PUBLIC_LIMITED_COMPANY = "public_limited_company"
    HOLDING_COMPANY = "holding_company"
    SUBSIDIARY = "subsidiary"
    ASSOCIATE = "associate"
    JOINT_VENTURE = "joint_venture"


class ShareClass(str, Enum):
    ORDINARY = "ordinary"
    PREFERENCE = "preference"
    REDEEMABLE = "redeemable"
    FOUNDERS = "founders"
    MANAGEMENT = "management"


class CapitalTransactionType(str, Enum):
    SHARE_ISSUANCE = "share_issuance"
    SHARE_REDEMPTION = "share_redemption"
    BONUS_ISSUE = "bonus_issue"
    RIGHTS_ISSUE = "rights_issue"
    CAPITAL_REDUCTION = "capital_reduction"
    DIVIDEND_PAYMENT = "dividend_payment"
    SHARE_PREMIUM = "share_premium"
    MERGER_CONTRIBUTION = "merger_contribution"


class DividendType(str, Enum):
    INTERIM = "interim"
    FINAL = "final"
    SPECIAL = "special"
    SCRIP = "scrip"
    CASH = "cash"


class CompanyStatus(str, Enum):
    ACTIVE = "active"
    DORMANT = "dormant"
    LIQUIDATION = "liquidation"
    ADMINISTRATION = "administration"
    DISSOLVED = "dissolved"


# ============================================================================
# Pydantic Models
# ============================================================================


class Company(BaseModel):
    id: str
    company_code: str
    company_name: str
    registration_number: Optional[str] = None
    company_type: CompanyType
    incorporation_date: datetime
    financial_year_end: str  # Month name
    registered_office: Optional[str] = None
    jurisdiction: str  # Country/State
    tax_id: Optional[str] = None
    status: CompanyStatus = CompanyStatus.ACTIVE
    accounting_standard: str = "IFRS"
    functional_currency: str = "USD"
    parent_company_id: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class Shareholder(BaseModel):
    id: str
    company_id: str
    shareholder_name: str
    shareholder_type: str  # individual, corporate, institutional
    share_class: ShareClass
    shares_held: int
    percentage_holding: float
    registration_date: datetime
    address: Optional[str] = None
    tax_status: Optional[str] = None
    is_controlling_party: bool = False
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ShareCapital(BaseModel):
    id: str
    company_id: str
    share_class: ShareClass
    authorized_shares: int
    issued_shares: int
    paid_up_value_per_share: Decimal
    total_paid_up_capital: Decimal
    share_premium: Decimal = Decimal("0")
    par_value: Optional[Decimal] = None
    currency: str = "USD"
    as_of_date: datetime
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CapitalTransaction(BaseModel):
    id: str
    company_id: str
    transaction_type: CapitalTransactionType
    transaction_date: datetime
    share_class: Optional[ShareClass] = None
    number_of_shares: int = 0
    price_per_share: Decimal
    total_amount: Decimal
    share_premium_amount: Optional[Decimal] = None
    reason: Optional[str] = None
    shareholder_id: Optional[str] = None
    reference_number: str
    approved_by: Optional[str] = None
    journal_entry_id: Optional[str] = None
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class Dividend(BaseModel):
    id: str
    company_id: str
    dividend_type: DividendType
    declaration_date: datetime
    record_date: datetime
    payment_date: Optional[datetime] = None
    per_share_amount: Decimal
    total_amount: Decimal
    currency: str = "USD"
    share_class: ShareClass = ShareClass.ORDINARY
    tax_withheld: Decimal = Decimal("0")
    net_payment: Decimal
    status: str = "declared"  # declared, approved, paid, cancelled
    approved_by: Optional[str] = None
    journal_entry_id: Optional[str] = None
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class DividendPayment(BaseModel):
    id: str
    dividend_id: str
    shareholder_id: str
    shareholder_name: str
    shares_held: int
    gross_amount: Decimal
    tax_withheld: Decimal
    net_amount: Decimal
    payment_date: Optional[datetime] = None
    payment_method: Optional[str] = None
    payment_reference: Optional[str] = None
    status: str = "pending"  # pending, processed, failed
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class RetainedEarnings(BaseModel):
    id: str
    company_id: str
    period_start: datetime
    period_end: datetime
    opening_balance: Decimal
    net_profit_for_period: Decimal
    dividends_declared: Decimal
    prior_year_adjustments: Decimal = Decimal("0")
    transfers_to_reserves: Decimal = Decimal("0")
    closing_balance: Decimal
    currency: str = "USD"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class Reserve(BaseModel):
    id: str
    company_id: str
    reserve_name: str
    reserve_type: str  # statutory, capital, revenue, general
    opening_balance: Decimal
    transfers_in: Decimal = Decimal("0")
    transfers_out: Decimal = Decimal("0")
    closing_balance: Decimal
    restriction_notes: Optional[str] = None
    currency: str = "USD"
    as_of_date: datetime
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class EquityReport(BaseModel):
    id: str
    company_id: str
    report_date: datetime
    share_capital: Decimal
    share_premium: Decimal
    reserves: Decimal
    retained_earnings: Decimal
    total_equity: Decimal
    movements: List[Dict[str, Any]] = []
    generated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


# ============================================================================
# Storage
# ============================================================================

companies: Dict[str, Company] = {}
shareholders: Dict[str, Shareholder] = {}
share_capitals: Dict[str, ShareCapital] = {}
capital_transactions: Dict[str, CapitalTransaction] = {}
dividends: Dict[str, Dividend] = {}
dividend_payments: Dict[str, DividendPayment] = {}
retained_earnings: Dict[str, List[RetainedEarnings]] = {}
reserves: Dict[str, List[Reserve]] = {}


# ============================================================================
# Internal API Helper Functions
# ============================================================================


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None):
    """Call accounting service for core accounting functions"""
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
    """Log to audit service"""
    async with httpx.AsyncClient() as client:
        url = f"{AUDIT_SERVICE_URL}/events"
        try:
            await client.post(url, json=event_data, timeout=5.0)
        except httpx.RequestError:
            pass


async def call_currency_service(method: str, endpoint: str, data: Optional[Dict] = None):
    """Call currency service for exchange rates"""
    async with httpx.AsyncClient() as client:
        url = f"{CURRENCY_SERVICE_URL}{endpoint}"
        try:
            if method == "GET":
                response = await client.get(url, timeout=10.0)
            elif method == "POST":
                response = await client.post(url, json=data, timeout=10.0)
            else:
                return {"error": "Method not supported"}
            return response.json()
        except httpx.RequestError:
            return {"error": "Currency service unavailable", "data": None}


# ============================================================================
# API Endpoints
# ============================================================================


@app.get("/")
async def health_check():
    """Health check endpoint"""
    return {
        "status": "healthy",
        "service": "company-accounting",
        "version": "1.0.0",
        "total_companies": len(companies),
        "total_shareholders": len(shareholders),
    }


# --- Company Management ---


@app.post("/companies")
async def create_company(company: Company):
    """Create a new company"""
    company.id = str(uuid.uuid4())
    company.created_at = datetime.now(timezone.utc)
    company.updated_at = datetime.now(timezone.utc)

    companies[company.id] = company

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
            "resource_id": company.id,
            "user_id": "system",
            "action_details": {"company_name": company.company_name, "type": company.company_type.value},
        }
    )

    return company


@app.get("/companies")
async def list_companies(company_type: Optional[CompanyType] = None, status: Optional[CompanyStatus] = None):
    """List all companies"""
    results = list(companies.values())

    if company_type:
        results = [c for c in results if c.company_type == company_type]
    if status:
        results = [c for c in results if c.status == status]

    return results


@app.get("/companies/{company_id}")
async def get_company(company_id: str):
    """Get company details"""
    if company_id not in companies:
        raise HTTPException(status_code=404, detail="Company not found")
    return companies[company_id]


@app.put("/companies/{company_id}")
async def update_company(company_id: str, company: Company):
    """Update company details"""
    if company_id not in companies:
        raise HTTPException(status_code=404, detail="Company not found")

    company.id = company_id
    company.updated_at = datetime.now(timezone.utc)
    companies[company_id] = company

    return company


# --- Shareholder Management ---


@app.post("/shareholders")
async def register_shareholder(shareholder: Shareholder):
    """Register a new shareholder"""
    shareholder.id = str(uuid.uuid4())
    shareholder.created_at = datetime.now(timezone.utc)

    shareholders[shareholder.id] = shareholder

    # Update percentage holdings for all shareholders in this company
    await recalculate_shareholding_percentages(shareholder.company_id)

    return shareholder


@app.get("/shareholders")
async def list_shareholders(company_id: Optional[str] = None, share_class: Optional[ShareClass] = None):
    """List shareholders"""
    results = list(shareholders.values())

    if company_id:
        results = [s for s in results if s.company_id == company_id]
    if share_class:
        results = [s for s in results if s.share_class == share_class]

    return results


async def recalculate_shareholding_percentages(company_id: str):
    """Recalculate percentage holdings for all shareholders"""
    company_shareholders = [s for s in shareholders.values() if s.company_id == company_id]
    total_shares = sum(s.shares_held for s in company_shareholders)

    for shareholder in company_shareholders:
        if total_shares > 0:
            shareholder.percentage_holding = (shareholder.shares_held / total_shares) * 100


# --- Share Capital Management ---


@app.post("/share-capital")
async def create_share_capital(share_capital: ShareCapital):
    """Create share capital record"""
    share_capital.id = str(uuid.uuid4())
    share_capital.created_at = datetime.now(timezone.utc)

    # Calculate totals
    share_capital.total_paid_up_capital = (
        Decimal(str(share_capital.issued_shares)) * share_capital.paid_up_value_per_share
    )

    share_capitals[share_capital.id] = share_capital

    return share_capital


@app.get("/share-capital")
async def get_share_capital(company_id: str):
    """Get current share capital for a company"""
    results = [sc for sc in share_capitals.values() if sc.company_id == company_id]
    return results[-1] if results else None


# --- Capital Transactions ---


@app.post("/capital-transactions")
async def record_capital_transaction(transaction: CapitalTransaction):
    """Record a capital transaction"""
    transaction.id = str(uuid.uuid4())
    transaction.created_at = datetime.now(timezone.utc)

    capital_transactions[transaction.id] = transaction

    # Create journal entry in accounting service
    company = companies.get(transaction.company_id)

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
            transaction.journal_entry_id = journal_result.get("id")

    return transaction


@app.get("/capital-transactions")
async def list_capital_transactions(
    company_id: Optional[str] = None,
    transaction_type: Optional[CapitalTransactionType] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
):
    """List capital transactions"""
    results = list(capital_transactions.values())

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


# --- Dividend Management ---


@app.post("/dividends")
async def declare_dividend(dividend: Dividend):
    """Declare a dividend"""
    dividend.id = str(uuid.uuid4())
    dividend.created_at = datetime.now(timezone.utc)
    dividend.net_payment = dividend.total_amount - dividend.tax_withheld

    dividends[dividend.id] = dividend

    # Generate individual dividend payments for each shareholder
    company_shareholders = [s for s in shareholders.values() if s.company_id == dividend.company_id]
    for shareholder in company_shareholders:
        if shareholder.share_class == dividend.share_class:
            shares = shareholder.shares_held
            gross = shares * dividend.per_share_amount
            tax = gross * Decimal("0.1")  # Assume 10% withholding
            net = gross - tax

            payment = DividendPayment(
                id=str(uuid.uuid4()),
                dividend_id=dividend.id,
                shareholder_id=shareholder.id,
                shareholder_name=shareholder.shareholder_name,
                shares_held=shares,
                gross_amount=gross,
                tax_withheld=tax,
                net_amount=net,
                created_at=datetime.now(timezone.utc),
            )
            dividend_payments[payment.id] = payment

    # Create journal entry
    company = companies.get(dividend.company_id)
    if company:
        await call_accounting_service(
            "POST",
            "/journal-entries/",
            {
                "description": f"Dividend declared - {dividend.dividend_type.value}",
                "reference": f"DIV-{dividend.id[:8]}",
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

    return dividend


@app.get("/dividends")
async def list_dividends(company_id: Optional[str] = None, status: Optional[str] = None):
    """List dividends"""
    results = list(dividends.values())

    if company_id:
        results = [d for d in results if d.company_id == company_id]
    if status:
        results = [d for d in results if d.status == status]

    return results


@app.post("/dividends/{dividend_id}/pay")
async def pay_dividend(dividend_id: str, approved_by: str):
    """Process dividend payment"""
    if dividend_id not in dividends:
        raise HTTPException(status_code=404, detail="Dividend not found")

    dividend = dividends[dividend_id]
    dividend.status = "paid"
    dividend.approved_by = approved_by

    # Update individual payments
    for payment in dividend_payments.values():
        if payment.dividend_id == dividend_id:
            payment.status = "processed"
            payment.payment_date = datetime.now(timezone.utc)

    # Create journal entry
    company = companies.get(dividend.company_id)
    if company:
        await call_accounting_service(
            "POST",
            "/journal-entries/",
            {
                "description": f"Dividend payment - {dividend.dividend_type.value}",
                "reference": f"DIV-PAY-{dividend.id[:8]}",
                "date": datetime.now().isoformat(),
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
async def get_dividend_payments(dividend_id: str):
    """Get individual dividend payments"""
    return [p for p in dividend_payments.values() if p.dividend_id == dividend_id]


# --- Retained Earnings ---


@app.post("/retained-earnings")
async def calculate_retained_earnings(entry: RetainedEarnings):
    """Calculate and record retained earnings"""
    entry.id = str(uuid.uuid4())
    entry.created_at = datetime.now(timezone.utc)

    # Calculate closing balance
    entry.closing_balance = (
        entry.opening_balance
        + entry.net_profit_for_period
        - entry.dividends_declared
        + entry.prior_year_adjustments
        - entry.transfers_to_reserves
    )

    if entry.company_id not in retained_earnings:
        retained_earnings[entry.company_id] = []
    retained_earnings[entry.company_id].append(entry)

    return entry


@app.get("/retained-earnings")
async def get_retained_earnings(company_id: str):
    """Get retained earnings history"""
    if company_id not in retained_earnings:
        return []
    return retained_earnings[company_id]


# --- Reserves ---


@app.post("/reserves")
async def create_reserve(reserve: Reserve):
    """Create or update a reserve"""
    reserve.id = str(uuid.uuid4())
    reserve.created_at = datetime.now(timezone.utc)
    reserve.closing_balance = reserve.opening_balance + reserve.transfers_in - reserve.transfers_out

    if reserve.company_id not in reserves:
        reserves[reserve.company_id] = []
    reserves[reserve.company_id].append(reserve)

    return reserve


@app.get("/reserves")
async def get_reserves(company_id: str):
    """Get all reserves for a company"""
    if company_id not in reserves:
        return []
    return reserves[company_id]


# --- Reports ---


@app.get("/reports/equity-statement/{company_id}")
async def get_equity_statement(company_id: str, as_of_date: datetime):
    """Generate statement of changes in equity"""
    if company_id not in companies:
        raise HTTPException(status_code=404, detail="Company not found")

    company = companies[company_id]

    # Get share capital
    share_capital = await get_share_capital(company_id)
    share_capital_amount = share_capital.total_paid_up_capital if share_capital else Decimal("0")
    share_premium_amount = share_capital.share_premium if share_capital else Decimal("0")

    # Get retained earnings
    re_entries = retained_earnings.get(company_id, [])
    current_re = re_entries[-1].closing_balance if re_entries else Decimal("0")

    # Get reserves
    company_reserves = reserves.get(company_id, [])
    total_reserves = sum(r.closing_balance for r in company_reserves)

    # Get movements for the period
    movements = []
    for transaction in capital_transactions.values():
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

    return EquityReport(
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
async def get_shareholder_register(company_id: str):
    """Generate shareholder register"""
    if company_id not in companies:
        raise HTTPException(status_code=404, detail="Company not found")

    company_shareholders = [s for s in shareholders.values() if s.company_id == company_id]

    # Sort by percentage holding
    company_shareholders.sort(key=lambda x: x.percentage_holding, reverse=True)

    return {
        "company": companies[company_id],
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
async def get_dividend_history(company_id: str):
    """Get dividend payment history"""
    company_dividends = [d for d in dividends.values() if d.company_id == company_id]
    company_dividends.sort(key=lambda x: x.declaration_date, reverse=True)

    return {
        "company_id": company_id,
        "total_dividends_declared": len(company_dividends),
        "total_amount": str(sum(d.total_amount for d in company_dividends)),
        "dividends": company_dividends,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8101)
