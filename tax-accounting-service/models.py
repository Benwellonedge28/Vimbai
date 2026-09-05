"""Pydantic models for Tax Accounting Service"""

import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field

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
    book_id: Optional[str] = None
    user_id: Optional[str] = None
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
    book_id: Optional[str] = None
    user_id: Optional[str] = None
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
    book_id: Optional[str] = None
    user_id: Optional[str] = None
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
    book_id: Optional[str] = None
    user_id: Optional[str] = None
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
    book_id: Optional[str] = None
    user_id: Optional[str] = None
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
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    period_end: datetime
    deferred_tax_asset: float = 0
    deferred_tax_liability: float = 0
    tax_rate: float
    temporary_differences: List[Dict[str, Any]] = []
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class TaxReport(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
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
