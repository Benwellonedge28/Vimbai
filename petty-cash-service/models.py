"""Pydantic models for Petty Cash Service (API contract unchanged)."""

from datetime import datetime, timezone
from decimal import Decimal
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


class PettyCashStatus(str, Enum):
    ACTIVE = "active"
    REPLENISHING = "replenishing"
    CLOSED = "closed"
    REIMBURSING = "reimbursing"


class TransactionType(str, Enum):
    RECEIPT = "receipt"
    PAYMENT = "payment"
    REPLENISHMENT = "replenishment"
    INITIAL_FUND = "initial_fund"
    ADJUSTMENT = "adjustment"
    CLOSING = "closing"


class PaymentCategory(str, Enum):
    TRAVEL = "travel"
    OFFICE_SUPPLIES = "office_supplies"
    POSTAGE = "postage"
    PRINTING = "printing"
    MEALS = "meals"
    TRANSPORTATION = "transportation"
    MISCELLANEOUS = "miscellaneous"
    STATIONERY = "stationery"
    TELEPHONE = "telephone"
    OFFICE_EXPENSES = "office_expenses"


class ReimbursementStatus(str, Enum):
    PENDING = "pending"
    APPROVED = "approved"
    REJECTED = "rejected"
    PROCESSED = "processed"
    CANCELLED = "cancelled"


# ============================================================================
# Pydantic Models
# ============================================================================


class PettyCashFund(BaseModel):
    id: str
    book_id: Optional[str] = None
    fund_code: str
    fund_name: str
    custodian_id: str
    custodian_name: str
    location: str
    maximum_balance: Decimal
    minimum_balance: Decimal
    replenishment_threshold: Decimal
    replenishment_amount: Decimal
    status: PettyCashStatus = PettyCashStatus.ACTIVE
    account_code: str  # Link to main accounting
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class PettyCashTransaction(BaseModel):
    id: str
    book_id: Optional[str] = None
    fund_id: str
    transaction_type: TransactionType
    amount: Decimal
    date: datetime
    description: str
    category: PaymentCategory
    recipient_name: Optional[str] = None
    recipient_id: Optional[str] = None
    reference_number: str
    voucher_number: str
    approved_by: Optional[str] = None
    entered_by: str
    receipt_attachment: Optional[str] = None
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class PettyCashReplenishment(BaseModel):
    id: str
    book_id: Optional[str] = None
    fund_id: str
    amount: Decimal
    request_date: datetime
    requested_by: str
    approved_by: Optional[str] = None
    approved_date: Optional[datetime] = None
    status: ReimbursementStatus = ReimbursementStatus.PENDING
    transactions_included: List[str] = []  # Transaction IDs
    total_cash_disbursed: Decimal = Decimal("0")
    bank_reference: Optional[str] = None
    notes: Optional[str] = None


class PettyCashSummary(BaseModel):
    fund_id: str
    fund_name: str
    opening_balance: Decimal
    total_receipts: Decimal
    total_payments: Decimal
    closing_balance: Decimal
    outstanding_vouchers: int
    available_cash: Decimal
    replenishment_needed: bool
    last_replenishment_date: Optional[datetime] = None


class PettyCashVoucher(BaseModel):
    id: str
    book_id: Optional[str] = None
    fund_id: str
    voucher_number: str
    date: datetime
    payee: str
    amount: Decimal
    description: str
    category: PaymentCategory
    approved_by: Optional[str] = None
    receipt_attached: bool = False
    status: str = "pending"
    entered_by: str
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
