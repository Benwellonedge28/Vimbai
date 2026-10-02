"""Pydantic models for Bank Feed Integration Service (API contract unchanged)."""

from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


class BankProvider(str, Enum):
    PLAID = "plaid"
    STRIPE = "stripe"
    QUICKBOOKS = "quickbooks"
    XERO = "xero"
    MANUAL = "manual"


class AccountType(str, Enum):
    CHECKING = "checking"
    SAVINGS = "savings"
    CREDIT_CARD = "credit_card"
    INVESTMENT = "investment"
    LOAN = "loan"
    MONEY_MARKET = "money_market"


class TransactionStatus(str, Enum):
    PENDING = "pending"
    CLEARED = "cleared"
    RECONCILED = "reconciled"
    DISPUTED = "disputed"
    RETURNED = "returned"


class SyncStatus(str, Enum):
    PENDING = "pending"
    SYNCING = "syncing"
    COMPLETED = "completed"
    FAILED = "failed"


class BankConnectionCreate(BaseModel):
    provider: BankProvider
    account_name: str
    account_type: AccountType
    account_number_last4: str = Field(..., max_length=4)
    routing_number: Optional[str] = None
    access_token_encrypted: Optional[str] = None  # Encrypted in production
    webhook_url: Optional[str] = None
    auto_sync_enabled: bool = True
    sync_interval_minutes: int = 60


class BankConnection(BankConnectionCreate):
    book_id: Optional[str] = None
    id: str
    organization_id: str
    status: str = "active"
    last_sync_at: Optional[datetime] = None
    last_sync_status: Optional[SyncStatus] = None
    error_message: Optional[str] = None
    created_at: datetime
    updated_at: datetime


class TransactionImport(BaseModel):
    bank_connection_id: str
    external_id: str
    date: datetime
    amount: float
    currency: str = "USD"
    description: str
    category: Optional[str] = None
    merchant_name: Optional[str] = None
    merchant_id: Optional[str] = None
    transaction_type: str = "debit"  # debit or credit
    pending: bool = False
    metadata: Optional[Dict[str, Any]] = None


class TransactionInDB(TransactionImport):
    book_id: Optional[str] = None
    id: str
    bank_connection_id: str
    linked_journal_entry_id: Optional[str] = None
    linked_invoice_id: Optional[str] = None
    matched_rule_id: Optional[str] = None
    status: TransactionStatus
    confidence_score: float = 0.0
    imported_at: datetime
    created_at: datetime
    updated_at: datetime


class ReconciliationRule(BaseModel):
    book_id: Optional[str] = None
    id: str
    name: str
    description: Optional[str] = None
    match_conditions: Dict[str, Any]  # conditions for auto-matching
    priority: int = 0
    auto_match_enabled: bool = True
    create_journal_entry: bool = False
    journal_entry_template: Optional[Dict[str, Any]] = None
    active: bool = True


class BankBalance(BaseModel):
    account_id: str
    available_balance: float
    current_balance: float
    currency: str = "USD"
    as_of_date: datetime
    pending_transactions: float = 0.0


class SyncRequest(BaseModel):
    bank_connection_id: str
    start_date: Optional[datetime] = None
    end_date: Optional[datetime] = None
    force_full_sync: bool = False


class SyncResult(BaseModel):
    book_id: Optional[str] = None
    sync_id: str
    bank_connection_id: str
    status: SyncStatus
    transactions_imported: int = 0
    transactions_updated: int = 0
    transactions_matched: int = 0
    errors: List[str] = []
    started_at: datetime
    completed_at: Optional[datetime] = None


BANK_PROVIDERS = {
    "plaid": {
        "name": "Plaid",
        "base_url": "https://production.plaid.com",
        "supports_balance": True,
        "supports_transactions": True,
        "supports_transfer": False,
    },
    "stripe": {
        "name": "Stripe",
        "base_url": "https://api.stripe.com",
        "supports_balance": True,
        "supports_transactions": False,
        "supports_transfer": True,
    },
    "quickbooks": {
        "name": "QuickBooks",
        "base_url": "https://quickbooks.api.intuit.com",
        "supports_balance": True,
        "supports_transactions": True,
        "supports_transfer": True,
    },
    "xero": {
        "name": "Xero",
        "base_url": "https://api.xero.com",
        "supports_balance": True,
        "supports_transactions": True,
        "supports_transfer": True,
    },
    "manual": {
        "name": "Manual Import",
        "base_url": None,
        "supports_balance": True,
        "supports_transactions": True,
        "supports_transfer": False,
    },
}
