from pydantic import BaseModel, Field, condecimal, validator
from typing import Optional, List, Literal, Dict, Any
from datetime import datetime
from decimal import Decimal

# --- Bank Models ---
class BankBase(BaseModel):
    name: str = Field(..., min_length=3, max_length=100, description="Name of the bank.")
    access_token: str = Field(..., description="Encrypted access token for the bank's API.")
    # Other bank-specific credentials

class BankCreate(BankBase):
    pass

class BankUpdate(BaseModel):
    name: Optional[str] = None
    access_token: Optional[str] = None

class BankInDB(BankBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this bank connection.")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Account Models (from bank perspective) ---
class BankAccountBase(BaseModel):
    bank_id: str = Field(..., description="ID of the connected bank.")
    account_name: str = Field(..., description="Name of the account (e.g., 'Checking', 'Savings').")
    account_number: str = Field(..., description="Bank account number (masked or sensitive, depending on security).")
    account_type: Literal["checking", "savings", "credit_card", "loan"] = Field(..., description="Type of bank account.")
    balance: condecimal(decimal_places=2) = Field(..., description="Current balance of the account.")
    currency: str = Field("USD", max_length=3, description="Currency of the account.")

    @validator('balance', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class BankAccountCreate(BankAccountBase):
    pass

class BankAccountUpdate(BaseModel):
    account_name: Optional[str] = None
    account_number: Optional[str] = None
    account_type: Optional[Literal["checking", "savings", "credit_card", "loan"]] = None
    balance: Optional[condecimal(decimal_places=2)] = None
    currency: Optional[str] = None

    @validator('balance', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class BankAccountInDB(BankAccountBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this bank account.")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Transaction Models ---
class TransactionBase(BaseModel):
    bank_account_id: str = Field(..., description="ID of the bank account this transaction belongs to.")
    transaction_id: str = Field(..., description="Unique ID provided by the bank/provider for this transaction.")
    description: str = Field(..., max_length=500, description="Description of the transaction.")
    amount: condecimal(decimal_places=2) = Field(..., description="Amount of the transaction. Positive for income, negative for expense.")
    currency: str = Field("USD", max_length=3, description="Currency of the transaction.")
    transaction_date: datetime = Field(..., description="Date of the transaction.")
    post_date: Optional[datetime] = Field(None, description="Date the transaction was posted by the bank.")
    category: Optional[str] = Field(None, max_length=100, description="Categorization of the transaction (e.g., 'Groceries', 'Salary').")
    accounting_account_number: Optional[str] = Field(None, description="Mapped account number in the Accounting Service's COA.")
    journal_entry_id: Optional[str] = Field(None, description="ID of the corresponding JournalEntry in the Accounting Service.")
    fraud_flag: Literal["safe", "low_risk", "suspicious", "high_risk"] = Field("safe", description="Flag from fraud detection service.") # NEW
    fraud_score: Optional[float] = Field(None, ge=0.0, le=1.0, description="Fraud score from fraud detection service.") # NEW


    @validator('amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class TransactionCreate(TransactionBase):
    pass

class TransactionUpdate(BaseModel):
    description: Optional[str] = None
    amount: Optional[condecimal(decimal_places=2)] = None
    currency: Optional[str] = None
    transaction_date: Optional[datetime] = None
    post_date: Optional[datetime] = None
    category: Optional[str] = None
    accounting_account_number: Optional[str] = None
    journal_entry_id: Optional[str] = None
    fraud_flag: Optional[Literal["safe", "low_risk", "suspicious", "high_risk"]] = None # NEW
    fraud_score: Optional[float] = Field(None, ge=0.0, le=1.0) # NEW

    @validator('amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class TransactionInDB(TransactionBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this transaction.")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Journal Entry Models (for inter-service communication) ---
class JournalLineBase(BaseModel):
    account_number: str
    debit: Decimal = Field(Decimal('0.00'))
    credit: Decimal = Field(Decimal('0.00'))
    description: Optional[str] = None

class JournalEntryCreate(BaseModel):
    entry_date: datetime = Field(default_factory=datetime.utcnow)
    description: str
    reference_number: Optional[str] = None
    source_module: str = "Banking"
    lines: List[JournalLineBase]
    status: Literal['pending', 'posted', 'reviewed', 'voided'] = Field('pending', description="Current status of the journal entry.")

class CreateJournalEntryResponse(BaseModel):
    status: str
    message: str
    journal_entry_id: Optional[str] = None

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
