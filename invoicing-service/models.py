from pydantic import BaseModel, Field, condecimal, validator # NEW: import condecimal, validator
from typing import Optional, List, Literal # NEW: import Literal
from datetime import datetime
from decimal import Decimal

# --- Customer Models ---
class CustomerBase(BaseModel):
    name: str = Field(..., min_length=3, max_length=100, description="Customer's name or company name.") # ADDED VALIDATION
    email: str = Field(..., example="contact@acmecorp.com", description="Customer's primary contact email.") # ADDED VALIDATION
    phone: Optional[str] = Field(None, max_length=20, description="Customer's phone number.") # ADDED VALIDATION
    address: Optional[str] = Field(None, max_length=255, description="Customer's billing address.") # ADDED VALIDATION
    customer_id: str = Field(..., min_length=3, max_length=50, regex=r"^[A-Z0-9-]+$", description="Unique identifier for the customer.") # ADDED VALIDATION

class CustomerCreate(CustomerBase):
    pass

class CustomerUpdate(BaseModel):
    name: Optional[str] = Field(None, min_length=3, max_length=100) # ADDED VALIDATION
    email: Optional[str] = Field(None, example="contact@acmecorp.com") # ADDED VALIDATION
    phone: Optional[str] = Field(None, max_length=20) # ADDED VALIDATION
    address: Optional[str] = Field(None, max_length=255) # ADDED VALIDATION

class CustomerInDB(CustomerBase):
    id: str = Field(..., example="uuid-string-for-node") # Neo4j node ID
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Invoice Item Models ---
class InvoiceItemBase(BaseModel):
    description: str = Field(..., min_length=3, max_length=500, description="Description of the service or product.") # ADDED VALIDATION
    quantity: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('1.00'), description="Quantity of the item.") # ADDED VALIDATION
    unit_price: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(..., description="Unit price of the item.") # ADDED VALIDATION
    amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(..., description="Total amount for this line item (quantity * unit_price).") # ADDED VALIDATION
    account_number: Optional[str] = Field(None, min_length=4, max_length=10, regex=r"^\d+$", description="Associated revenue account number from Accounting Service.") # ADDED VALIDATION

    @validator('quantity', 'unit_price', 'amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v
    
    @validator('amount')
    def validate_amount(cls, v, values):
        quantity = values.get('quantity')
        unit_price = values.get('unit_price')
        if quantity is not None and unit_price is not None:
            calculated_amount = quantity * unit_price
            if v != calculated_amount:
                # Allow slight floating point discrepancies, but flag major ones
                if abs(v - calculated_amount) > Decimal('0.01'):
                    raise ValueError(f"Amount ({v}) does not match quantity ({quantity}) * unit_price ({unit_price}) = {calculated_amount}")
        return v

class InvoiceItemCreate(InvoiceItemBase):
    pass

class InvoiceItemInDB(InvoiceItemBase):
    id: str = Field(..., example="uuid-string-for-node") # Neo4j node ID
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Invoice Models ---
class InvoiceBase(BaseModel):
    customer_id: str = Field(..., min_length=3, description="ID of the customer this invoice is for.") # ADDED VALIDATION
    invoice_number: str = Field(..., min_length=3, max_length=50, regex=r"^[A-Z0-9-]+$", description="Unique invoice number.") # ADDED VALIDATION
    invoice_date: datetime = Field(default_factory=datetime.utcnow, description="Date the invoice was issued.")
    due_date: datetime = Field(..., description="Date the payment is due.")
    total_amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(..., description="Total amount of the invoice.") # ADDED VALIDATION
    status: Literal["draft", "outstanding", "paid", "overdue", "void"] = Field("draft", description="Current status of the invoice.") # ADDED VALIDATION (Literal)
    notes: Optional[str] = Field(None, max_length=1000, description="Any additional notes for the invoice.") # ADDED VALIDATION
    items: List[InvoiceItemCreate] = Field(..., min_length=1, description="List of items included in the invoice.") # ADDED VALIDATION

    @validator('total_amount')
    def validate_total_amount(cls, v, values):
        items = values.get('items')
        if items:
            calculated_total = sum(item.amount for item in items)
            if v != calculated_total:
                if abs(v - calculated_total) > Decimal('0.01'):
                    raise ValueError(f"Total amount ({v}) does not match sum of item amounts ({calculated_total})")
        return v
    
    @validator('due_date')
    def validate_due_date(cls, v, values):
        invoice_date = values.get('invoice_date')
        if invoice_date and v < invoice_date:
            raise ValueError("Due date cannot be before invoice date.")
        return v

class InvoiceCreate(InvoiceBase):
    pass

class InvoiceUpdate(BaseModel):
    invoice_date: Optional[datetime] = None
    due_date: Optional[datetime] = None
    total_amount: Optional[condecimal(decimal_places=2, ge=Decimal('0.00'))] = None
    status: Optional[Literal["draft", "outstanding", "paid", "overdue", "void"]] = None
    notes: Optional[str] = Field(None, max_length=1000)
    # Updating items would typically be handled by separate endpoints or a more complex update logic

class InvoiceInDB(InvoiceBase):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    items: List[InvoiceItemInDB] # Items retrieved will have their own IDs

    class Config:
        from_attributes = True

# --- Journal Entry Models (for inter-service communication) ---
# Copied from accounting-service for definition consistency.
class JournalLineBase(BaseModel):
    account_number: str
    debit: Decimal = Field(Decimal('0.00'))
    credit: Decimal = Field(Decimal('0.00'))
    description: Optional[str] = None

class JournalEntryCreate(BaseModel):
    entry_date: datetime = Field(default_factory=datetime.utcnow)
    description: str
    reference_number: Optional[str] = None
    source_module: str = "Invoicing"
    lines: List[JournalLineBase]

class CreateJournalEntryResponse(BaseModel):
    status: str
    message: str
    journal_entry_id: Optional[str] = None

# --- Error Response Model (NEW) ---
class ErrorResponse(BaseModel):
    detail: str
    code: Optional[str] = None
    status_code: int = 500
