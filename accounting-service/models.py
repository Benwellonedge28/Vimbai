from pydantic import BaseModel, Field
from typing import Optional, List
from datetime import datetime

# --- Chart of Accounts Models ---
class AccountBase(BaseModel):
    account_number: str = Field(..., example="1010", description="Unique numerical identifier for the account.")
    account_name: str = Field(..., example="Cash (Bank Account)", description="Descriptive name of the account.")
    account_type: str = Field(..., example="Asset", description="Type of account (Asset, Liability, Equity, Revenue, Expense).")
    normal_balance: str = Field(..., example="Debit", description="Debit or Credit, indicating how increases are recorded.")
    description: Optional[str] = Field(None, example="Main operating bank account.", description="Detailed description of the account.")
    parent_account_number: Optional[str] = Field(None, example="1000", description="The account number of the parent account for hierarchical grouping.")

class AccountCreate(AccountBase):
    pass

class AccountUpdate(BaseModel):
    account_name: Optional[str] = None
    account_type: Optional[str] = None
    normal_balance: Optional[str] = None
    description: Optional[str] = None
    parent_account_number: Optional[str] = None

class AccountInDB(AccountBase):
    id: str = Field(..., example="uuid-string-for-node") # Neo4j node ID or UUID
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Journal Entry and Line Models (Placeholders for future) ---
class JournalLineBase(BaseModel):
    account_number: str
    debit_amount: Optional[float] = 0.0
    credit_amount: Optional[float] = 0.0
    description: Optional[str] = None

class JournalEntryBase(BaseModel):
    entry_date: datetime = Field(default_factory=datetime.utcnow)
    description: str
    reference_number: Optional[str] = None
    source_module: Optional[str] = "Manual"
    lines: List[JournalLineBase]
