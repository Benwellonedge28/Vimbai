from pydantic import BaseModel, Field, condecimal, validator
from typing import Optional, List, Literal, Dict, Any, Union
from datetime import datetime
from decimal import Decimal

# --- Account Models ---
class AccountBase(BaseModel):
    name: str = Field(..., min_length=3, max_length=100, description="Name of the account (e.g., 'Cash', 'Accounts Receivable', 'Salaries Expense').")
    account_number: str = Field(..., min_length=4, max_length=10, regex=r"^\d+$", description="Unique identifier for the account.")
    account_type: Literal["asset", "liability", "equity", "revenue", "expense"] = Field(..., description="Type of the account based on accounting equation.")
    normal_balance: Literal["debit", "credit"] = Field(..., description="The side (debit or credit) on which increases in the account are recorded.")
    description: Optional[str] = Field(None, max_length=500, description="Detailed description of the account's purpose.")
    parent_account_number: Optional[str] = Field(None, description="The account number of the parent account in a hierarchical chart of accounts.")

class AccountCreate(AccountBase):
    pass

class AccountUpdate(BaseModel):
    name: Optional[str] = Field(None, min_length=3, max_length=100)
    account_type: Optional[Literal["asset", "liability", "equity", "revenue", "expense"]] = None
    normal_balance: Optional[Literal["debit", "credit"]] = None
    description: Optional[str] = Field(None, max_length=500)
    parent_account_number: Optional[str] = Field(None, description="The account number of the parent account in a hierarchical chart of accounts.")

class AccountInDB(AccountBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this account.")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Journal Entry Models ---
class JournalLineBase(BaseModel):
    account_number: str = Field(..., description="The account number impacted by this line item.")
    debit: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('0.00'), description="Debit amount.")
    credit: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('0.00'), description="Credit amount.")
    description: Optional[str] = Field(None, max_length=500, description="Description specific to this journal line.")

    @validator('debit', 'credit', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v
    
    @validator('debit')
    def validate_debit_credit_exclusive(cls, v, values):
        if v > 0 and values.get('credit', Decimal('0.00')) > 0:
            raise ValueError("A journal line cannot have both debit and credit amounts.")
        return v
    
    @validator('credit')
    def validate_credit_debit_exclusive(cls, v, values):
        if v > 0 and values.get('debit', Decimal('0.00')) > 0:
            raise ValueError("A journal line cannot have both debit and credit amounts.")
        return v

class JournalEntryCreate(BaseModel):
    entry_date: datetime = Field(default_factory=datetime.utcnow, description="Date of the journal entry.")
    description: str = Field(..., max_length=1000, description="Overall description of the journal entry.")
    reference_number: Optional[str] = Field(None, max_length=100, description="Optional reference number (e.g., invoice number, check number).")
    source_module: str = Field(..., max_length=50, description="Module that generated this entry (e.g., 'Invoicing', 'Banking', 'Manual').")
    lines: List[JournalLineBase] = Field(..., min_length=2, description="List of journal lines for this entry. Must balance.")
    status: Literal['pending', 'posted', 'reviewed', 'voided'] = Field('pending', description="Current status of the journal entry.")
    
    @validator('lines')
    def validate_lines_balance(cls, v):
        total_debits = sum(line.debit for line in v)
        total_credits = sum(line.credit for line in v)
        if total_debits != total_credits:
            raise ValueError(f"Journal entry is out of balance. Debits: {total_debits}, Credits: {total_credits}.")
        return v

class JournalEntryUpdate(BaseModel):
    entry_date: Optional[datetime] = None
    description: Optional[str] = Field(None, max_length=1000)
    reference_number: Optional[str] = Field(None, max_length=100)
    source_module: Optional[str] = Field(None, max_length=50)
    status: Optional[Literal['pending', 'posted', 'reviewed', 'voided']] = None
    # Lines are not updated directly via this model; would require separate logic or a new endpoint

class JournalEntryInDB(JournalEntryCreate):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this journal entry.")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    fraud_flag: Literal["safe", "low_risk", "suspicious", "high_risk"] = Field("safe", description="Flag from fraud detection service.") # NEW
    fraud_score: Optional[float] = Field(None, ge=0.0, le=1.0, description="Fraud score from fraud detection service.") # NEW

    class Config:
        from_attributes = True

# --- Ledger Models ---
class LedgerEntry(BaseModel):
    entry_id: str = Field(..., description="ID of the associated journal entry.")
    entry_date: datetime = Field(..., description="Date of the journal entry.")
    description: str = Field(..., description="Description from the journal entry.")
    debit: Decimal = Field(Decimal('0.00'), description="Debit amount.")
    credit: Decimal = Field(Decimal('0.00'), description="Credit amount.")
    balance: Decimal = Field(..., description="Running balance of the account after this entry.")
    source_module: str = Field(..., description="Module that generated this entry.")

class LedgerReport(BaseModel):
    account_number: str
    account_name: str
    normal_balance: Literal["debit", "credit"]
    start_balance: Decimal = Field(Decimal('0.00'))
    entries: List[LedgerEntry]
    end_balance: Decimal = Field(Decimal('0.00'))

# --- Trial Balance Models ---
class TrialBalanceAccount(BaseModel):
    account_number: str
    account_name: str
    account_type: Literal["asset", "liability", "equity", "revenue", "expense"]
    debit: Decimal = Field(Decimal('0.00'))
    credit: Decimal = Field(Decimal('0.00'))

class TrialBalanceReport(BaseModel):
    report_date: datetime = Field(default_factory=datetime.utcnow)
    accounts: List[TrialBalanceAccount]
    total_debits: Decimal = Field(Decimal('0.00'))
    total_credits: Decimal = Field(Decimal('0.00'))
    is_balanced: bool = Field(False)

# --- Financial Statement Models ---
class FinancialStatementLine(BaseModel):
    category: str
    amount: Decimal

class IncomeStatement(BaseModel):
    start_date: datetime
    end_date: datetime
    revenues: List[FinancialStatementLine]
    expenses: List[FinancialStatementLine]
    net_income: Decimal

class BalanceSheet(BaseModel):
    as_of_date: datetime
    assets: List[FinancialStatementLine]
    liabilities: List[FinancialStatementLine]
    equity: List[FinancialStatementLine]
    total_assets: Decimal
    total_liabilities: Decimal
    total_equity: Decimal
    total_liabilities_equity: Decimal # Should equal total_assets

# --- Fraud Detection Service Models (for inter-service communication) ---
class TransactionForFraudCheck(BaseModel): # Copied from fraud-detection-service/models.py
    transaction_id: str = Field(..., description="Unique ID of the transaction.")
    amount: condecimal(decimal_places=2, gt=Decimal('0.00')) = Field(..., description="Amount of the transaction.")
    currency: str = Field("USD", max_length=3, description="Currency of the transaction (ISO 4217).")
    sender_account_id: str = Field(..., description="ID of the sender's account.")
    recipient_account_id: str = Field(..., description="ID of the recipient's account.")
    transaction_type: Literal["debit", "credit", "transfer", "payment", "purchase"] = Field(..., description="Type of transaction.")
    timestamp: datetime = Field(default_factory=datetime.utcnow, description="Timestamp of the transaction.")
    location_data: Optional[Dict[str, Any]] = Field(None, description="Geographic location data of the transaction.")
    device_info: Optional[Dict[str, Any]] = Field(None, description="Device information (e.g., IP address, OS, browser).")
    previous_transactions_count_24h: int = Field(0, ge=0, description="Number of transactions by sender in last 24h.")
    avg_daily_transaction_amount_7d: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('0.00'), description="Average daily transaction amount by sender over 7 days.")

    @validator('amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class FraudDetectionResult(BaseModel): # Copied from fraud-detection-service/models.py
    transaction_id: str = Field(..., description="ID of the transaction that was analyzed.")
    fraud_score: float = Field(..., ge=0.0, le=1.0, description="Probability or score indicating likelihood of fraud (0-1).")
    fraud_flag: Literal["safe", "low_risk", "suspicious", "high_risk"] = Field(..., description="Categorical flag for fraud risk.")
    reason: Optional[str] = Field(None, description="Reason or rules triggered for the flag.")
    model_version: str = Field(..., description="Version of the ML model used for detection.")

# --- Error Response Model ---
class ErrorResponse(BaseModel):
    detail: str
    code: Optional[str] = None
    status_code: int = 500
