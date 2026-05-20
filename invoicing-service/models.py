from pydantic import BaseModel, Field
from typing import Optional, List
from datetime import datetime
from decimal import Decimal

# --- Customer Models ---
class CustomerBase(BaseModel):
    name: str = Field(..., example="Acme Corp", description="Customer's name or company name.")
    email: str = Field(..., example="contact@acmecorp.com", description="Customer's primary contact email.")
    phone: Optional[str] = Field(None, example="+15551234567", description="Customer's phone number.")
    address: Optional[str] = Field(None, example="123 Main St, Anytown, CA", description="Customer's billing address.")
    customer_id: str = Field(..., example="CUST001", description="Unique identifier for the customer.")

class CustomerCreate(CustomerBase):
    pass

class CustomerUpdate(BaseModel):
    name: Optional[str] = None
    email: Optional[str] = None
    phone: Optional[str] = None
    address: Optional[str] = None

class CustomerInDB(CustomerBase):
    id: str = Field(..., example="uuid-string-for-node") # Neo4j node ID
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Invoice Item Models ---
class InvoiceItemBase(BaseModel):
    description: str = Field(..., example="Consulting services", description="Description of the service or product.")
    quantity: Decimal = Field(Decimal('1.00'), ge=Decimal('0.00'), description="Quantity of the item.")
    unit_price: Decimal = Field(..., ge=Decimal('0.00'), description="Unit price of the item.")
    amount: Decimal = Field(..., ge=Decimal('0.00'), description="Total amount for this line item (quantity * unit_price).")
    account_number: Optional[str] = Field(None, example="4000", description="Associated revenue account number from Accounting Service.")

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
    customer_id: str = Field(..., example="CUST001", description="ID of the customer this invoice is for.")
    invoice_number: str = Field(..., example="INV-2026-0001", description="Unique invoice number.")
    invoice_date: datetime = Field(default_factory=datetime.utcnow, description="Date the invoice was issued.")
    due_date: datetime = Field(..., description="Date the payment is due.")
    total_amount: Decimal = Field(..., ge=Decimal('0.00'), description="Total amount of the invoice.")
    status: str = Field("draft", example="outstanding", description="Current status of the invoice (draft, outstanding, paid, overdue, void).")
    notes: Optional[str] = Field(None, description="Any additional notes for the invoice.")
    items: List[InvoiceItemCreate] = Field(..., min_length=1, description="List of items included in the invoice.")

class InvoiceCreate(InvoiceBase):
    pass

class InvoiceUpdate(BaseModel):
    invoice_date: Optional[datetime] = None
    due_date: Optional[datetime] = None
    total_amount: Optional[Decimal] = None
    status: Optional[str] = None
    notes: Optional[str] = None
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
