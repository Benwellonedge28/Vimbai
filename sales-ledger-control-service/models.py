"""Pydantic models for Sales Ledger Control Service (API contract unchanged)."""

import uuid
from datetime import datetime
from enum import Enum
from typing import Optional

from pydantic import BaseModel, Field


class TransactionType(str, Enum):
    INVOICE = "invoice"
    CREDIT_NOTE = "credit_note"
    PAYMENT = "payment"
    REFUND = "refund"
    BAD_DEBT = "bad_debt"


class DebtorTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    transaction_type: TransactionType
    debtor_id: str
    debtor_name: str
    invoice_number: Optional[str] = None
    date: datetime
    amount: float
    balance: float = 0
    reference: Optional[str] = None
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ControlAccountSummary(BaseModel):
    as_of_date: datetime
    total_invoices: float = 0
    total_credit_notes: float = 0
    total_payments: float = 0
    total_bad_debts: float = 0
    closing_balance: float = 0
    transaction_count: int = 0
    debtor_count: int = 0
