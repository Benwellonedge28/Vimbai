"""Pydantic models for Cashbook Service"""

from datetime import datetime, timezone
from decimal import Decimal
from enum import Enum
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


class CashBookType(str, Enum):
    RECEIPTS = "receipts"
    PAYMENTS = "payments"
    CASH_JOURNAL = "cash_journal"


class TransactionStatus(str, Enum):
    PENDING = "pending"
    POSTED = "posted"
    RECONCILED = "reconciled"
    VOIDED = "voided"


class BankAccountType(str, Enum):
    CASH = "cash"
    BANK = "bank"
    SAVINGS = "savings"
    CURRENT = "current"


class BankAccount(BaseModel):
    id: Optional[str] = None
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    account_code: str
    account_name: str
    account_type: BankAccountType
    bank_name: Optional[str] = None
    account_number: Optional[str] = None
    currency: str = "USD"
    opening_balance: Decimal = Decimal("0")
    current_balance: Decimal = Decimal("0")
    is_active: bool = True
    reconciliation_enabled: bool = True
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CashBookEntry(BaseModel):
    id: Optional[str] = None
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    book_type: CashBookType
    entry_date: datetime
    voucher_number: str
    description: str
    account_code: str
    amount: Decimal
    is_debit: bool  # True for receipts/cash in, False for payments/cash out
    reference: Optional[str] = None
    cheque_number: Optional[str] = None
    bank_account: Optional[str] = None
    currency: str = "USD"
    exchange_rate: Decimal = Decimal("1.0")
    base_amount: Optional[Decimal] = None  # Amount in base currency
    narration: Optional[str] = None
    posted_by: str
    status: TransactionStatus = TransactionStatus.PENDING
    reconciled: bool = False
    reconciliation_id: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CashBookSummary(BaseModel):
    account_code: str
    account_name: str
    opening_balance: Decimal
    total_debits: Decimal
    total_credits: Decimal
    closing_balance: Decimal
    transaction_count: int
    last_transaction_date: Optional[datetime] = None


class BankReconciliation(BaseModel):
    id: Optional[str] = None
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    bank_account: str
    statement_date: datetime
    statement_balance: Decimal
    book_balance: Optional[Decimal] = None
    adjustments: List[Dict[str, Any]] = []
    adjusted_balance: Optional[Decimal] = None
    differences: List[Dict[str, Any]] = []
    status: str = "in_progress"  # in_progress, completed, reviewed
    prepared_by: str
    reviewed_by: Optional[str] = None
    completed_at: Optional[datetime] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CashFlowEntry(BaseModel):
    id: Optional[str] = None
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    entry_date: datetime
    category: str
    subcategory: Optional[str] = None
    description: str
    expected_amount: Optional[Decimal] = None
    actual_amount: Optional[Decimal] = None
    variance: Optional[Decimal] = None
    cash_flow_type: str  # operating, investing, financing
    source: str  # cash_receipts, cash_payments, bank
    reference_id: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CashPosition(BaseModel):
    as_of_date: datetime
    total_cash: Decimal
    total_bank: Decimal
    total_savings: Decimal
    total_current: Decimal
    by_currency: Dict[str, str]
    by_account: List[Dict[str, Any]]
