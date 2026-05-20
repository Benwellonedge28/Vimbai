from pydantic import BaseModel, Field
from typing import Optional, List
from datetime import datetime
from decimal import Decimal

class BankAccountBase(BaseModel):
    bank_name: str = Field(..., example="FinTech Bank", description="Name of the bank.")
    account_name: str = Field(..., example="Checking Account", description="User-friendly name for the account.")
    account_id: str = Field(..., example="ACC12345", description="Unique ID provided by the bank/integration.")
    account_type: str = Field(..., example="checking", description="Type of account (e.g., checking, savings, credit_card).")
    currency: str = Field("USD", example="USD", description="Currency of the account.")
    current_balance: Decimal = Field(Decimal('0.00'), description="Current balance of the account.")
    is_synced: bool = Field(False, description="Whether this account is actively synced.")
    last_synced_at: Optional[datetime] = Field(None, description="Timestamp of the last successful sync.")

class BankAccountCreate(BankAccountBase):
    pass

class BankAccountUpdate(BaseModel):
    bank_name: Optional[str] = None
    account_name: Optional[str] = None
    account_type: Optional[str] = None
    currency: Optional[str] = None
    current_balance: Optional[Decimal] = None
    is_synced: Optional[bool] = None
    last_synced_at: Optional[datetime] = None

class BankAccountInDB(BankAccountBase):
    id: str = Field(..., example="uuid-string-for-node") # Neo4j node ID or UUID
    user_id: str = Field(..., description="ID of the user who owns this bank account.")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

class BankTransactionBase(BaseModel):
    transaction_id: str = Field(..., example="TXN67890", description="Unique ID from the bank for this transaction.")
    date: datetime = Field(..., description="Date of the transaction.")
    description: str = Field(..., example="Payment to FinTech Solutions", description="Description of the transaction.")
    amount: Decimal = Field(..., description="Amount of the transaction (positive for inflow, negative for outflow).")
    transaction_type: str = Field(..., example="debit", description="Type of transaction (e.g., debit, credit, transfer).")
    category: Optional[str] = Field(None, example="Software Subscriptions", description="Categorization of the transaction.")
    reconciled: bool = Field(False, description="Whether this transaction has been reconciled in accounting.")

class BankTransactionCreate(BankTransactionBase):
    pass

class BankTransactionInDB(BankTransactionBase):
    id: str = Field(..., example="uuid-string-for-node")
    bank_account_id: str = Field(..., description="ID of the associated bank account.")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True
