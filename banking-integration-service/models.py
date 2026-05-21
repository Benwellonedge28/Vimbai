from pydantic import BaseModel, Field, condecimal
from typing import Optional, List, Dict, Any, Literal
from datetime import datetime, date

class BankConnectionBase(BaseModel):
    provider: str = Field(..., description="Bank API provider (e.g., Plaid, Yodlee, Direct Integration).", min_length=1)
    access_token: str = Field(..., description="Encrypted access token for the bank API.")
    external_id: str = Field(..., description="Provider-specific ID for the connection or institution.")
    status: Literal["active", "inactive", "reauth_required", "error"] = Field("active")
    last_synced_at: Optional[datetime] = None
    metadata: Dict[str, Any] = Field({}, description="Provider-specific metadata.")

class BankConnectionCreate(BankConnectionBase):
    user_id: str = Field(..., description="The user ID to whom this connection belongs.")

class BankConnectionUpdate(BaseModel):
    access_token: Optional[str] = None
    status: Optional[Literal["active", "inactive", "reauth_required", "error"]] = None
    last_synced_at: Optional[datetime] = None
    metadata: Optional[Dict[str, Any]] = None

class BankConnectionInDB(BankConnectionBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

class BankAccountBase(BaseModel):
    account_id: str = Field(..., description="Provider-specific ID for the bank account.")
    connection_id: str = Field(..., description="ID of the BankConnection it belongs to.")
    name: str = Field(..., description="Account name (e.g., Checking, Savings, Credit Card).", min_length=1)
    mask: str = Field(..., description="Last 4 digits of the account number.")
    type: str = Field(..., description="Account type (e.g., depository, credit).", min_length=1)
    subtype: str = Field(..., description="Account subtype (e.g., checking, savings, commercial credit card).", min_length=1)
    currency: str = Field(..., max_length=3, description="ISO 4217 currency code (e.g., USD, ZAR).", example="USD")
    current_balance: condecimal(max_digits=18, decimal_places=2) = Field(..., description="Current balance of the account.")
    available_balance: Optional[condecimal(max_digits=18, decimal_places=2)] = Field(None, description="Available balance (if different from current).")
    finacc_account_number: Optional[str] = Field(None, description="Corresponding account number in FinAcc Accounting Service.")
    status: Literal["active", "inactive", "closed"] = Field("active")

class BankAccountCreate(BankAccountBase):
    pass

class BankAccountUpdate(BaseModel):
    name: Optional[str] = None
    type: Optional[str] = None
    subtype: Optional[str] = None
    current_balance: Optional[condecimal(max_digits=18, decimal_places=2)] = None
    available_balance: Optional[condecimal(max_digits=18, decimal_places=2)] = None
    finacc_account_number: Optional[str] = None
    status: Optional[Literal["active", "inactive", "closed"]] = None

class BankAccountInDB(BankAccountBase):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

class BankTransactionBase(BaseModel):
    transaction_id: str = Field(..., description="Provider-specific ID for the transaction.")
    account_id: str = Field(..., description="ID of the BankAccount it belongs to.")
    description: str = Field(..., description="Transaction description from the bank.")
    amount: condecimal(max_digits=18, decimal_places=2) = Field(..., description="Transaction amount (positive for debit, negative for credit).", multiple_of=Decimal("0.01"))
    date: date = Field(..., description="Date of the transaction.")
    posted_date: Optional[date] = Field(None, description="Date transaction was posted.")
    category: Optional[str] = Field(None, description="AI-categorized transaction category.")
    type: Optional[str] = Field(None, description="Transaction type (e.g., ACH, card_payment, check, transfer).")
    status: Literal["pending", "posted", "cancelled"] = Field("posted")
    finacc_journal_entry_id: Optional[str] = Field(None, description="ID of the corresponding JournalEntry in FinAcc.")
    is_reconciled: bool = False
    metadata: Dict[str, Any] = Field({}, description="Provider-specific transaction metadata.")

class BankTransactionCreate(BankTransactionBase):
    pass

class BankTransactionUpdate(BaseModel):
    description: Optional[str] = None
    category: Optional[str] = None
    finacc_journal_entry_id: Optional[str] = None
    is_reconciled: Optional[bool] = None
    metadata: Optional[Dict[str, Any]] = None

class BankTransactionInDB(BankTransactionBase):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

class TransactionCategorizationRuleBase(BaseModel):
    user_id: str = Field(..., description="User ID to whom this rule belongs.")
    rule_name: str = Field(..., min_length=3, max_length=100, description="Name of the categorization rule.")
    match_field: Literal["description", "payee", "amount_range"] = Field(..., description="Field to match against.")
    match_pattern: str = Field(..., description="Regex pattern or keyword to match.")
    target_category: str = Field(..., description="Category to assign (e.g., 'Utilities', 'Travel').")
    target_finacc_account_number: Optional[str] = Field(None, description="Optional: FinAcc account number to suggest for JE.")
    is_active: bool = True
    priority: int = Field(0, description="Lower number means higher priority.")

class TransactionCategorizationRuleCreate(TransactionCategorizationRuleBase):
    pass

class TransactionCategorizationRuleUpdate(BaseModel):
    rule_name: Optional[str] = None
    match_field: Optional[Literal["description", "payee", "amount_range"]] = None
    match_pattern: Optional[str] = None
    target_category: Optional[str] = None
    target_finacc_account_number: Optional[str] = None
    is_active: Optional[bool] = None
    priority: Optional[int] = None

class TransactionCategorizationRuleInDB(TransactionCategorizationRuleBase):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

class ReconciliationMatchBase(BaseModel):
    bank_transaction_id: str = Field(..., description="ID of the BankTransaction.")
    finacc_journal_entry_id: str = Field(..., description="ID of the JournalEntry in FinAcc.")
    match_type: Literal["exact", "fuzzy", "manual"] = Field(..., description="How the match was made.")
    matched_amount: condecimal(max_digits=18, decimal_places=2) = Field(..., description="Amount that matched.")
    matched_date: date = Field(..., description="Date of the match.")
    is_confirmed: bool = False
    confirmed_by_user_id: Optional[str] = None
    confirmed_at: Optional[datetime] = None

class ReconciliationMatchCreate(ReconciliationMatchBase):
    pass

class ReconciliationMatchUpdate(BaseModel):
    match_type: Optional[Literal["exact", "fuzzy", "manual"]] = None
    is_confirmed: Optional[bool] = None
    confirmed_by_user_id: Optional[str] = None
    confirmed_at: Optional[datetime] = None

class ReconciliationMatchInDB(ReconciliationMatchBase):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True
