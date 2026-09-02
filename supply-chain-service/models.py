from datetime import datetime
from decimal import Decimal
from typing import Any, Dict, List, Literal, Optional

from pydantic import BaseModel, Field, condecimal, validator


# --- Supplier Models (NEW) ---
class SupplierBase(BaseModel):
    name: str = Field(..., min_length=3, max_length=100, description="Name of the supplier.")
    contact_person: Optional[str] = Field(None, max_length=100, description="Main contact person at the supplier.")
    email: Optional[str] = Field(None, description="Supplier's contact email.")
    phone: Optional[str] = Field(None, description="Supplier's contact phone number.")
    address: Optional[str] = Field(None, max_length=255, description="Supplier's physical address.")
    tax_id: Optional[str] = Field(None, max_length=50, description="Supplier's tax identification number.")


class SupplierCreate(SupplierBase):
    pass


class SupplierUpdate(BaseModel):
    name: Optional[str] = Field(None, min_length=3, max_length=100)
    contact_person: Optional[str] = Field(None, max_length=100)
    email: Optional[str] = Field(None)
    phone: Optional[str] = Field(None)
    address: Optional[str] = Field(None, max_length=255)
    tax_id: Optional[str] = Field(None, max_length=50)


class SupplierInDB(SupplierBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this supplier record.")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


# --- Inventory Item Models (NEW) ---
class InventoryItemBase(BaseModel):
    name: str = Field(..., min_length=3, max_length=100, description="Name of the inventory item.")
    sku: str = Field(..., min_length=1, max_length=50, description="Stock Keeping Unit.")
    description: Optional[str] = Field(None, max_length=500, description="Description of the item.")
    unit_cost: condecimal(decimal_places=2, ge=Decimal("0.00")) = Field(..., description="Cost per unit.")
    unit_of_measure: str = Field(..., max_length=20, description="Unit of measure (e.g., 'pcs', 'kg', 'liters').")
    current_stock: int = Field(0, ge=0, description="Current quantity in stock.")
    reorder_point: Optional[int] = Field(None, ge=0, description="Minimum stock level before reordering.")
    preferred_supplier_id: Optional[str] = Field(None, description="ID of the preferred supplier for this item.")

    @validator("unit_cost", pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v


class InventoryItemCreate(InventoryItemBase):
    pass


class InventoryItemUpdate(BaseModel):
    name: Optional[str] = Field(None, min_length=3, max_length=100)
    sku: Optional[str] = Field(None, min_length=1, max_length=50)
    description: Optional[str] = Field(None, max_length=500)
    unit_cost: Optional[condecimal(decimal_places=2, ge=Decimal("0.00"))] = None
    unit_of_measure: Optional[str] = Field(None, max_length=20)
    current_stock: Optional[int] = Field(None, ge=0)
    reorder_point: Optional[int] = Field(None, ge=0)
    preferred_supplier_id: Optional[str] = Field(None)

    @validator("unit_cost", pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v


class InventoryItemInDB(InventoryItemBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this inventory item.")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


# --- Purchase Order Models (NEW) ---
class PurchaseOrderItemBase(BaseModel):
    inventory_item_id: str = Field(..., description="ID of the inventory item being ordered.")
    quantity: int = Field(..., ge=1, description="Quantity of the item being ordered.")
    unit_price: condecimal(decimal_places=2, ge=Decimal("0.00")) = Field(
        ..., description="Agreed unit price for this item on the PO."
    )
    line_total: condecimal(decimal_places=2, ge=Decimal("0.00")) = Field(
        ..., description="Calculated total for this line item (quantity * unit_price)."
    )

    @validator("unit_price", "line_total", pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

    @validator("line_total")
    def validate_line_total(cls, v, values):
        if "quantity" in values and "unit_price" in values:
            expected_total = values["quantity"] * values["unit_price"]
            if v != expected_total:
                raise ValueError("Line total does not match quantity * unit price.")
        return v


class PurchaseOrderBase(BaseModel):
    supplier_id: str = Field(..., description="ID of the supplier for this purchase order.")
    order_date: datetime = Field(default_factory=datetime.utcnow, description="Date the purchase order was issued.")
    expected_delivery_date: Optional[datetime] = Field(None, description="Expected date of delivery.")
    total_amount: condecimal(decimal_places=2, ge=Decimal("0.00")) = Field(
        ..., description="Total amount of the purchase order."
    )
    currency: str = Field("USD", max_length=3, description="Currency of the purchase order.")
    status: Literal[
        "draft", "pending_approval", "approved", "ordered", "received", "cancelled", "partially_received"
    ] = Field("draft", description="Current status of the purchase order.")
    notes: Optional[str] = Field(
        None, max_length=1000, description="Any notes or special instructions for the supplier."
    )
    items: List[PurchaseOrderItemBase] = Field(
        ..., min_length=1, description="List of items included in this purchase order."
    )

    @validator("total_amount", pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

    @validator("total_amount")
    def validate_total_amount(cls, v, values):
        if "items" in values:
            calculated_total = sum(item.line_total for item in values["items"])
            if v != calculated_total:
                raise ValueError("Total amount does not match sum of line items.")
        return v

    @validator("due_date")
    def validate_due_date(cls, v, values):
        if "invoice_date" in values and v < values["invoice_date"]:
            raise ValueError("Due date cannot be before invoice date.")
        return v


class PurchaseOrderCreate(PurchaseOrderBase):
    pass


class PurchaseOrderUpdate(BaseModel):
    supplier_id: Optional[str] = None
    order_date: Optional[datetime] = None
    expected_delivery_date: Optional[datetime] = None
    total_amount: Optional[condecimal(decimal_places=2, ge=Decimal("0.00"))] = None
    currency: Optional[str] = None
    status: Optional[
        Literal["draft", "pending_approval", "approved", "ordered", "received", "cancelled", "partially_received"]
    ] = None
    notes: Optional[str] = Field(None, max_length=1000)
    # Items update would require specific sub-endpoints or a more complex model

    @validator("total_amount", pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v


class PurchaseOrderInDB(PurchaseOrderBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this purchase order.")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    items: List[PurchaseOrderItemBase] = []  # Redefine items to include internal ID if needed

    class Config:
        from_attributes = True


# --- Customer Models (from original Invoicing Service, unchanged) ---
class CustomerBase(BaseModel):
    name: str = Field(..., min_length=3, max_length=100, description="Full name of the customer or company name.")
    email: Optional[str] = Field(None, description="Customer's contact email.")
    phone: Optional[str] = Field(None, description="Customer's contact phone number.")
    address: Optional[str] = Field(None, max_length=255, description="Customer's billing address.")
    tax_id: Optional[str] = Field(None, max_length=50, description="Customer's tax identification number.")


class CustomerCreate(CustomerBase):
    pass


class CustomerUpdate(BaseModel):
    name: Optional[str] = Field(None, min_length=3, max_length=100)
    email: Optional[str] = Field(None)
    phone: Optional[str] = Field(None)
    address: Optional[str] = Field(None, max_length=255)
    tax_id: Optional[str] = Field(None, max_length=50)


class CustomerInDB(CustomerBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this customer record.")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


# --- Sales Invoice Models (renamed from InvoiceBase) ---
class SalesInvoiceItemBase(BaseModel):
    description: str = Field(..., max_length=500, description="Description of the service or product.")
    quantity: int = Field(..., ge=1, description="Quantity of the item/service.")
    unit_price: condecimal(decimal_places=2, ge=Decimal("0.00")) = Field(..., description="Price per unit.")
    line_total: condecimal(decimal_places=2, ge=Decimal("0.00")) = Field(
        ..., description="Calculated total for this line item (quantity * unit_price)."
    )

    @validator("unit_price", "line_total", pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

    @validator("line_total")
    def validate_line_total(cls, v, values):
        if "quantity" in values and "unit_price" in values:
            expected_total = values["quantity"] * values["unit_price"]
            if v != expected_total:
                raise ValueError("Line total does not match quantity * unit price.")
        return v


class SalesInvoiceBase(BaseModel):
    customer_id: str = Field(..., description="ID of the customer this invoice is for.")
    invoice_date: datetime = Field(default_factory=datetime.utcnow, description="Date the invoice was issued.")
    due_date: datetime = Field(..., description="Date payment is due.")
    total_amount: condecimal(decimal_places=2, ge=Decimal("0.00")) = Field(
        ..., description="Total amount of the invoice."
    )
    currency: str = Field("USD", max_length=3, description="Currency of the invoice.")
    status: Literal["draft", "sent", "paid", "overdue", "void"] = Field(
        "draft", description="Current status of the invoice."
    )
    items: List[SalesInvoiceItemBase] = Field(..., min_length=1, description="List of items included in this invoice.")
    notes: Optional[str] = Field(None, max_length=1000, description="Any notes or special instructions.")

    @validator("total_amount", pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

    @validator("total_amount")
    def validate_total_amount(cls, v, values):
        if "items" in values:
            calculated_total = sum(item.line_total for item in values["items"])
            if v != calculated_total:
                raise ValueError("Total amount does not match sum of line items.")
        return v

    @validator("due_date")
    def validate_due_date(cls, v, values):
        if "invoice_date" in values and v < values["invoice_date"]:
            raise ValueError("Due date cannot be before invoice date.")
        return v


class SalesInvoiceCreate(SalesInvoiceBase):
    pass


class SalesInvoiceUpdate(BaseModel):
    customer_id: Optional[str] = None
    invoice_date: Optional[datetime] = None
    due_date: Optional[datetime] = None
    total_amount: Optional[condecimal(decimal_places=2, ge=Decimal("0.00"))] = None
    currency: Optional[str] = None
    status: Optional[Literal["draft", "sent", "paid", "overdue", "void"]] = None
    notes: Optional[str] = Field(None, max_length=1000)

    @validator("total_amount", pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

    @validator("due_date")
    def validate_due_date(cls, v, values):
        if "invoice_date" in values and v < values["invoice_date"]:
            raise ValueError("Due date cannot be before invoice date.")
        return v


class SalesInvoiceInDB(SalesInvoiceBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this invoice.")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    items: List[SalesInvoiceItemBase] = []

    class Config:
        from_attributes = True


# --- Error Response Model (unchanged) ---
class ErrorResponse(BaseModel):
    detail: str
    code: Optional[str] = None
    status_code: int = 500
