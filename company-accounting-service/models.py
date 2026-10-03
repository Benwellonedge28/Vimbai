"""Pydantic models for Company Accounting Service.

Vimbai record-keeping only: this service stores company registry,
share capital, dividend, retained earnings and reserve records.
It never moves money; journal entries are delegated to the accounting
service and failures are tolerated.
"""

from datetime import datetime, timezone
from decimal import Decimal
from enum import Enum
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


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
