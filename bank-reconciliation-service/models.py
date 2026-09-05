"""Pydantic models for Bank Reconciliation Service"""

import uuid
from datetime import datetime
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


class TransactionType(str, Enum):
    DEPOSIT = "deposit"
    WITHDRAWAL = "withdrawal"
    CHEQUE = "cheque"
    DIRECT_DEBIT = "direct_debit"
    DIRECT_CREDIT = "direct_credit"
    TRANSFER = "transfer"
    BANK_CHARGE = "bank_charge"
    INTEREST = "interest"
    STANDING_ORDER = "standing_order"


class MatchStatus(str, Enum):
    MATCHED = "matched"
    UNMATCHED = "unmatched"
    PARTIAL_MATCH = "partial_match"
    DISPUTED = "disputed"


class BankStatementLine(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    statement_id: str
    date: datetime
    description: str
    reference: Optional[str] = None
    transaction_type: TransactionType
    amount: float
    balance: float
    is_debit: bool
    matched: bool = False
    matched_transaction_id: Optional[str] = None


class BankStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    bank_account: str
    statement_number: str
    statement_start_date: datetime
    statement_end_date: datetime
    opening_balance: float
    closing_balance: float
    total_credits: float = 0
    total_debits: float = 0
    lines: List[BankStatementLine] = []
    status: str = "imported"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CashBookEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    date: datetime
    description: str
    reference: str
    transaction_type: str
    amount: float
    is_debit: bool
    bank_reconciliation_id: Optional[str] = None
    matched: bool = False
    matched_statement_id: Optional[str] = None


class ReconciliationItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    item_type: str  # "bank_only" or "cash_book_only"
    date: datetime
    description: str
    reference: Optional[str] = None
    amount: float
    match_status: MatchStatus = MatchStatus.UNMATCHED
    suggested_match_id: Optional[str] = None
    notes: Optional[str] = None


class BankReconciliation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    bank_account: str
    reconciliation_date: datetime
    statement_balance: float
    cash_book_balance: float
    difference: float = 0

    # Adjustments
    outstanding_deposits: float = 0
    outstanding_cheques: float = 0
    bank_errors: float = 0
    cash_book_errors: float = 0
    unpresented_cheques: float = 0
    uncredited_deposits: float = 0

    # Final balances
    adjusted_statement_balance: float = 0
    adjusted_cash_book_balance: float = 0

    items: List[ReconciliationItem] = []
    status: str = "in_progress"
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    completed_at: Optional[datetime] = None
