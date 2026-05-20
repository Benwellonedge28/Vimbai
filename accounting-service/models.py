from pydantic import BaseModel, Field, condecimal, model_validator
from typing import Optional, List
from datetime import datetime
from decimal import Decimal # Use Decimal for financial calculations to avoid floating point issues

# --- Chart of Accounts Models (unchanged) ---
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

# --- Journal Entry and Line Models ---

class JournalLineBase(BaseModel):
    account_number: str = Field(..., example="1010", description="Account number affected by this line.")
    debit: condecimal(ge=Decimal('0.00'), decimal_places=2) = Field(Decimal('0.00'), description="Debit amount.")
    credit: condecimal(ge=Decimal('0.00'), decimal_places=2) = Field(Decimal('0.00'), description="Credit amount.")
    description: Optional[str] = Field(None, example="Payment for services.", description="Description of the journal line.")

    # Validation: one and only one of debit/credit must be non-zero
    @model_validator(mode='after')
    def check_debit_credit(self) -> 'JournalLineBase':
        if (self.debit > 0 and self.credit > 0) or \
           (self.debit == 0 and self.credit == 0):
            raise ValueError("Exactly one of 'debit' or 'credit' must be a positive amount.")
        return self

class JournalEntryBase(BaseModel):
    entry_date: datetime = Field(default_factory=datetime.utcnow, description="Date of the journal entry.")
    description: str = Field(..., example="Record monthly utility bill.", description="Overall description of the entry.")
    reference_number: Optional[str] = Field(None, example="INV-2023-001", description="Reference number (e.g., invoice number, check number).")
    source_module: str = Field("Manual", example="Manual", description="Module from which the entry originated (e.g., 'Manual', 'POS', 'Multimodal').")
    lines: List[JournalLineBase] = Field(..., min_length=2, description="List of journal lines. Must have at least two lines for double-entry.")

    # Validation: Debits must equal Credits (double-entry principle)
    @model_validator(mode='after')
    def check_balanced_entry(self) -> 'JournalEntryBase':
        total_debit = sum(line.debit for line in self.lines)
        total_credit = sum(line.credit for line in self.lines)
        if total_debit != total_credit:
            raise ValueError("Journal entry is not balanced: Total Debits must equal Total Credits.")
        return self

class JournalEntryCreate(JournalEntryBase):
    pass

class JournalEntryUpdate(BaseModel):
    entry_date: Optional[datetime] = None
    description: Optional[str] = None
    reference_number: Optional[str] = None
    source_module: Optional[str] = None
    lines: Optional[List[JournalLineBase]] = None # Updating lines would be complex, often involves re-creating or specific adjustments.

class JournalEntryInDB(JournalEntryBase):
    id: str = Field(..., example="uuid-string-for-node") # Neo4j node ID or UUID
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True
