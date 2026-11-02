"""Pydantic models for the Control Account Reconciliation Service.

:ReconciliationError via :OWNS_RECONCILIATION_ERROR and :ReconciliationReport via
:OWNS_RECONCILIATION_REPORT, both Book-gated. A report keeps the snapshot of the
pending errors it found as a JSON string.
"""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class ControlAccountType(str, Enum):
    SALES_LEDGER = "sales_ledger"
    PURCHASES_LEDGER = "purchases_ledger"
    BILLS_RECEIVABLE = "bills_receivable"
    BILLS_PAYABLE = "bills_payable"


class ErrorType(str, Enum):
    DUPLICATE_ENTRY = "duplicate_entry"
    POSTING_ERROR = "posting_error"
    MISSING_ENTRY = "missing_entry"
    AMOUNT_MISMATCH = "amount_mismatch"
    TIMING_DIFFERENCE = "timing_difference"
    TRANSFER_ERROR = "transfer_error"
    ROUNDING_ERROR = "rounding_error"


class ReconciliationError(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    error_type: ErrorType
    control_account_type: ControlAccountType
    transaction_id: Optional[str] = None
    description: str
    amount: float = 0
    expected_amount: float = 0
    variance: float = 0
    status: str = "pending"  # pending, resolved, written_off
    resolution: Optional[str] = None
    resolved_by: Optional[str] = None
    resolved_at: Optional[datetime] = None
    created_at: datetime = Field(default_factory=_utcnow)


class ReconciliationReport(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    as_of_date: datetime
    control_account_type: ControlAccountType
    control_balance: float = 0
    ledger_total: float = 0
    difference: float = 0
    error_count: int = 0
    errors: List[ReconciliationError] = []
    status: str = "pending"
    created_at: datetime = Field(default_factory=_utcnow)
