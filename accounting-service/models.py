from pydantic import BaseModel, Field, condecimal, validator
from typing import Optional, List, Literal
from datetime import datetime
from decimal import Decimal

# ... (existing Account models) ...

# --- Journal Entry Models ---
class JournalLineBase(BaseModel):
    account_number: str = Field(..., min_length=4, max_length=10, regex=r"^\d+$", description="The number of the account being debited or credited.") # ADDED VALIDATION
    debit: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('0.00'), description="Debit amount, non-negative.") # ADDED VALIDATION
    credit: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('0.00'), description="Credit amount, non-negative.") # ADDED VALIDATION
    description: Optional[str] = Field(None, max_length=255, description="Specific description for this line item.")

    @validator('debit', 'credit', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v
    
    @validator('debit', 'credit')
    def validate_debit_credit(cls, v, values, field):
        if field.name == 'debit':
            credit = values.get('credit')
            if v > 0 and credit > 0:
                raise ValueError('A journal line cannot have both debit and credit amounts.')
        return v


class JournalEntryCreate(BaseModel):
    entry_date: datetime = Field(default_factory=datetime.utcnow, description="Date of the journal entry.")
    description: str = Field(..., min_length=5, max_length=500, description="Overall description of the journal entry.") # ADDED VALIDATION
    reference_number: Optional[str] = Field(None, max_length=50, description="Optional reference number (e.g., invoice number, check number).")
    source_module: str = Field("Manual", max_length=50, description="Source system or module that created the entry (e.g., 'Manual', 'Invoicing', 'Multimodal').")
    lines: List[JournalLineBase] = Field(..., min_length=2, description="List of journal lines. Must have at least two lines for double-entry.") # ADDED VALIDATION
    status: Literal['pending', 'posted', 'reviewed', 'voided'] = Field('pending', description="Current status of the journal entry.") # NEW STATUS FIELD

    @validator('lines')
    def must_be_balanced(cls, lines):
        total_debit = sum(line.debit for line in lines)
        total_credit = sum(line.credit for line in lines)
        if total_debit != total_credit:
            raise ValueError(f"Journal entry is unbalanced. Debits: {total_debit}, Credits: {total_credit}")
        return lines

class JournalEntryUpdate(BaseModel):
    entry_date: Optional[datetime] = None
    description: Optional[str] = None
    reference_number: Optional[str] = None
    source_module: Optional[str] = None
    # Lines should not be updated directly after creation for audit purposes; new JE for corrections
    status: Optional[Literal['pending', 'posted', 'reviewed', 'voided']] = None # Allow status update

class JournalEntryInDB(JournalEntryCreate):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# ... (existing Ledger, Trial Balance, Financial Statement models, ErrorResponse) ...
