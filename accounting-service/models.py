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

# --- Supply Chain Service Models (for inter-service communication) ---
# New base model for PurchaseOrderItem to match what is returned from SC Service CRUD
class PurchaseOrderItemBase(BaseModel):
    inventory_item_id: str
    quantity: int
    unit_price: condecimal(decimal_places=2, ge=Decimal('0.00'))
    line_total: condecimal(decimal_places=2, ge=Decimal('0.00'))

    @validator('unit_price', 'line_total', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

# New base model for PurchaseOrder to match what is returned from SC Service CRUD
class PurchaseOrderBase(BaseModel):
    supplier_id: str
    order_date: datetime
    expected_delivery_date: Optional[datetime]
    total_amount: condecimal(decimal_places=2, ge=Decimal('0.00'))
    currency: str
    status: Literal["draft", "pending_approval", "approved", "ordered", "received", "cancelled", "partially_received"]
    notes: Optional[str]
    items: List[PurchaseOrderItemBase]

    @validator('total_amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class PurchaseOrderInDB(PurchaseOrderBase):
    id: str
    user_id: str
    created_at: datetime
    updated_at: datetime

    class Config:
        from_attributes = True

# --- New model for creating Vendor Bills ---
class VendorBillCreate(BaseModel):
    purchase_order_id: str = Field(..., description="ID of the associated Purchase Order.")
    bill_date: datetime = Field(default_factory=datetime.utcnow, description="Date the vendor bill was received/recorded.")
    due_date: datetime = Field(..., description="Date payment for the bill is due.")
    # Optionally, allow overriding or adding additional lines not from PO
    additional_lines: Optional[List[JournalLineBase]] = Field(None, description="Additional journal lines not derived from the PO (e.g., shipping, taxes).")

class VendorBillInDB(BaseModel):
    id: str = Field(..., example="uuid-string-for-node")
    purchase_order_id: str
    bill_date: datetime
    due_date: datetime
    journal_entry_id: str
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Special Journals Models (Books of Original Entry) ---

class SalesJournalEntryBase(BaseModel):
    """Records credit sales of goods - Book of Original Entry"""
    invoice_number: str = Field(..., max_length=50, description="Sales invoice number")
    customer_id: str = Field(..., description="Customer ID from identity service")
    invoice_date: datetime = Field(default_factory=datetime.utcnow, description="Date of sale")
    due_date: Optional[datetime] = Field(None, description="Payment due date")
    total_amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(..., description="Total invoice amount")
    tax_amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('0.00'), description="Tax amount")
    discount_amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('0.00'), description="Discount amount")
    currency: str = Field("USD", max_length=3, description="Currency code (ISO 4217)")
    status: Literal['pending', 'invoiced', 'paid', 'cancelled', 'returned'] = Field('pending', description="Payment status")
    notes: Optional[str] = Field(None, max_length=500, description="Additional notes")

    @validator('total_amount', 'tax_amount', 'discount_amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class SalesJournalEntryCreate(SalesJournalEntryBase):
    pass

class SalesJournalEntryInDB(SalesJournalEntryBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this entry")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    journal_entry_id: Optional[str] = Field(None, description="Linked journal entry ID")

    class Config:
        from_attributes = True

# --- Purchases Journal ---
class PurchasesJournalEntryBase(BaseModel):
    """Records credit purchases of goods - Book of Original Entry"""
    purchase_order_number: str = Field(..., max_length=50, description="Purchase order number")
    vendor_id: str = Field(..., description="Vendor ID")
    purchase_date: datetime = Field(default_factory=datetime.utcnow, description="Date of purchase")
    due_date: Optional[datetime] = Field(None, description="Payment due date")
    total_amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(..., description="Total purchase amount")
    tax_amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('0.00'), description="Tax amount")
    discount_amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('0.00'), description="Discount amount")
    currency: str = Field("USD", max_length=3, description="Currency code (ISO 4217)")
    status: Literal['pending', 'ordered', 'received', 'paid', 'cancelled', 'returned'] = Field('pending', description="Purchase status")
    notes: Optional[str] = Field(None, max_length=500, description="Additional notes")

    @validator('total_amount', 'tax_amount', 'discount_amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class PurchasesJournalEntryCreate(PurchasesJournalEntryBase):
    pass

class PurchasesJournalEntryInDB(PurchasesJournalEntryBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this entry")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    journal_entry_id: Optional[str] = Field(None, description="Linked journal entry ID")

    class Config:
        from_attributes = True

# --- Cash Receipts Journal ---
class CashReceiptsJournalEntryBase(BaseModel):
    """Records all cash coming into the business - Book of Original Entry"""
    receipt_number: str = Field(..., max_length=50, description="Cash receipt number")
    customer_id: Optional[str] = Field(None, description="Customer ID (for customer payments)")
    receipt_date: datetime = Field(default_factory=datetime.utcnow, description="Date of receipt")
    amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(..., description="Cash amount received")
    payment_method: Literal['cash', 'check', 'bank_transfer', 'card', 'other'] = Field('cash', description="Payment method")
    reference_number: Optional[str] = Field(None, max_length=100, description="Reference/cheque number")
    bank_account: str = Field("1000", max_length=20, description="Cash/Bank account number")
    description: str = Field(..., max_length=500, description="Description of cash receipt")
    source_type: Literal['cash_sale', 'customer_payment', 'refund', 'loan', 'investment', 'other'] = Field('other', description="Source of cash receipt")
    status: Literal['pending', 'posted', 'reversed'] = Field('posted', description="Receipt status")

    @validator('amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class CashReceiptsJournalEntryCreate(CashReceiptsJournalEntryBase):
    pass

class CashReceiptsJournalEntryInDB(CashReceiptsJournalEntryBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this entry")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    journal_entry_id: Optional[str] = Field(None, description="Linked journal entry ID")

    class Config:
        from_attributes = True

# --- Cash Disbursements Journal ---
class CashDisbursementsJournalEntryBase(BaseModel):
    """Records all cash going out of the business - Book of Original Entry"""
    payment_number: str = Field(..., max_length=50, description="Payment/cheque number")
    vendor_id: Optional[str] = Field(None, description="Vendor ID (for supplier payments)")
    employee_id: Optional[str] = Field(None, description="Employee ID (for payroll)")
    payment_date: datetime = Field(default_factory=datetime.utcnow, description="Date of payment")
    amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(..., description="Cash amount paid")
    payment_method: Literal['cash', 'check', 'bank_transfer', 'card', 'other'] = Field('check', description="Payment method")
    reference_number: Optional[str] = Field(None, max_length=100, description="Reference/cheque number")
    bank_account: str = Field("1000", max_length=20, description="Cash/Bank account number")
    description: str = Field(..., max_length=500, description="Description of cash payment")
    expense_account: str = Field(..., max_length=20, description="Expense account to be debited")
    source_type: Literal['supplier_payment', 'salary', 'utility', 'rent', 'loan_repayment', 'refund', 'other'] = Field('other', description="Source of cash payment")
    status: Literal['pending', 'posted', 'cancelled'] = Field('posted', description="Payment status")

    @validator('amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class CashDisbursementsJournalEntryCreate(CashDisbursementsJournalEntryBase):
    pass

class CashDisbursementsJournalEntryInDB(CashDisbursementsJournalEntryBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this entry")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    journal_entry_id: Optional[str] = Field(None, description="Linked journal entry ID")

    class Config:
        from_attributes = True

# --- Sales Returns Journal ---
class SalesReturnsJournalEntryBase(BaseModel):
    """Records goods returned by customers - Book of Original Entry"""
    return_number: str = Field(..., max_length=50, description="Return merchandise authorization (RMA) number")
    original_invoice_number: str = Field(..., max_length=50, description="Original sales invoice number")
    customer_id: str = Field(..., description="Customer ID")
    return_date: datetime = Field(default_factory=datetime.utcnow, description="Date of return")
    total_amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(..., description="Total return amount")
    tax_amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('0.00'), description="Tax amount on return")
    reason: str = Field(..., max_length=200, description="Reason for return")
    status: Literal['pending', 'approved', 'received', 'refunded', 'rejected'] = Field('pending', description="Return status")
    notes: Optional[str] = Field(None, max_length=500, description="Additional notes")

    @validator('total_amount', 'tax_amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class SalesReturnsJournalEntryCreate(SalesReturnsJournalEntryBase):
    pass

class SalesReturnsJournalEntryInDB(SalesReturnsJournalEntryBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this entry")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    journal_entry_id: Optional[str] = Field(None, description="Linked journal entry ID")

    class Config:
        from_attributes = True

# --- Purchases Returns Journal ---
class PurchasesReturnsJournalEntryBase(BaseModel):
    """Records goods returned to suppliers - Book of Original Entry"""
    return_number: str = Field(..., max_length=50, description="Return number")
    original_po_number: str = Field(..., max_length=50, description="Original purchase order number")
    vendor_id: str = Field(..., description="Vendor ID")
    return_date: datetime = Field(default_factory=datetime.utcnow, description="Date of return")
    total_amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(..., description="Total return amount")
    tax_amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('0.00'), description="Tax amount on return")
    reason: str = Field(..., max_length=200, description="Reason for return")
    status: Literal['pending', 'approved', 'shipped', 'completed', 'rejected'] = Field('pending', description="Return status")
    notes: Optional[str] = Field(None, max_length=500, description="Additional notes")

    @validator('total_amount', 'tax_amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class PurchasesReturnsJournalEntryCreate(PurchasesReturnsJournalEntryBase):
    pass

class PurchasesReturnsJournalEntryInDB(PurchasesReturnsJournalEntryBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this entry")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    journal_entry_id: Optional[str] = Field(None, description="Linked journal entry ID")

    class Config:
        from_attributes = True

# --- Subsidiary Ledger Models ---

class AccountsReceivableLedgerEntry(BaseModel):
    """Detailed breakdown for AR - Subsidiary Ledger"""
    customer_id: str = Field(..., description="Customer ID")
    customer_name: str = Field(..., max_length=200, description="Customer name")
    invoice_number: str = Field(..., max_length=50, description="Invoice number")
    invoice_date: datetime = Field(..., description="Invoice date")
    due_date: datetime = Field(..., description="Payment due date")
    invoice_amount: Decimal = Field(..., description="Total invoice amount")
    balance_due: Decimal = Field(..., description="Current balance due")
    amount_paid: Decimal = Field(Decimal('0.00'), description="Amount paid so far")
    status: Literal['open', 'partial', 'paid', 'overdue', 'write_off'] = Field('open', description="Invoice status")
    days_outstanding: int = Field(0, ge=0, description="Days since invoice date")

class AccountsReceivableLedgerReport(BaseModel):
    """Complete AR Subsidiary Ledger"""
    as_of_date: datetime = Field(default_factory=datetime.utcnow)
    entries: List[AccountsReceivableLedgerEntry] = []
    total_invoice_amount: Decimal = Field(Decimal('0.00'))
    total_balance_due: Decimal = Field(Decimal('0.00'))
    total_amount_paid: Decimal = Field(Decimal('0.00'))
    customer_count: int = Field(0)
    overdue_count: int = Field(0)

class AccountsPayableLedgerEntry(BaseModel):
    """Detailed breakdown for AP - Subsidiary Ledger"""
    vendor_id: str = Field(..., description="Vendor ID")
    vendor_name: str = Field(..., max_length=200, description="Vendor name")
    bill_number: str = Field(..., max_length=50, description="Bill/invoice number")
    bill_date: datetime = Field(..., description="Bill date")
    due_date: datetime = Field(..., description="Payment due date")
    bill_amount: Decimal = Field(..., description="Total bill amount")
    balance_due: Decimal = Field(..., description="Current balance due")
    amount_paid: Decimal = Field(Decimal('0.00'), description="Amount paid so far")
    status: Literal['open', 'partial', 'paid', 'overdue'] = Field('open', description="Bill status")
    days_outstanding: int = Field(0, ge=0, description="Days since bill date")

class AccountsPayableLedgerReport(BaseModel):
    """Complete AP Subsidiary Ledger"""
    as_of_date: datetime = Field(default_factory=datetime.utcnow)
    entries: List[AccountsPayableLedgerEntry] = []
    total_bill_amount: Decimal = Field(Decimal('0.00'))
    total_balance_due: Decimal = Field(Decimal('0.00'))
    total_amount_paid: Decimal = Field(Decimal('0.00'))
    vendor_count: int = Field(0)
    overdue_count: int = Field(0)

class FixedAssetLedgerEntry(BaseModel):
    """Fixed Assets Subsidiary Ledger"""
    asset_id: str = Field(..., description="Fixed asset ID")
    asset_name: str = Field(..., max_length=200, description="Asset name")
    asset_category: str = Field(..., max_length=100, description="Asset category (Equipment, Vehicle, etc.)")
    purchase_date: datetime = Field(..., description="Date of purchase")
    purchase_cost: Decimal = Field(..., description="Original purchase cost")
    salvage_value: Decimal = Field(Decimal('0.00'), description="Estimated salvage/residual value")
    useful_life_years: int = Field(..., description="Estimated useful life in years")
    depreciation_method: Literal['straight_line', 'declining_balance', 'units_of_production'] = Field('straight_line', description="Depreciation method")
    accumulated_depreciation: Decimal = Field(Decimal('0.00'), description="Total depreciation to date")
    net_book_value: Decimal = Field(..., description="Current book value (Purchase cost - Accumulated depreciation)")
    location: Optional[str] = Field(None, max_length=200, description="Asset location")
    responsible_person: Optional[str] = Field(None, max_length=200, description="Person responsible for asset")
    status: Literal['active', 'disposed', 'under_repair', 'retired'] = Field('active', description="Asset status")

class FixedAssetsLedgerReport(BaseModel):
    """Complete Fixed Assets Subsidiary Ledger"""
    as_of_date: datetime = Field(default_factory=datetime.utcnow)
    entries: List[FixedAssetLedgerEntry] = []
    total_purchase_cost: Decimal = Field(Decimal('0.00'))
    total_accumulated_depreciation: Decimal = Field(Decimal('0.00'))
    total_net_book_value: Decimal = Field(Decimal('0.00'))
    asset_count: int = Field(0)

class InventoryLedgerEntry(BaseModel):
    """Inventory Subsidiary Ledger"""
    item_id: str = Field(..., description="Inventory item ID")
    item_name: str = Field(..., max_length=200, description="Item name")
    sku: str = Field(..., max_length=50, description="Stock keeping unit")
    category: str = Field(..., max_length=100, description="Item category")
    unit_of_measure: str = Field(..., max_length=20, description="Unit of measure (pcs, kg, etc.)")
    opening_quantity: int = Field(0, description="Opening stock quantity")
    stock_in_quantity: int = Field(0, description="Total stock received")
    stock_out_quantity: int = Field(0, description="Total stock sold/used")
    closing_quantity: int = Field(0, description="Closing stock quantity")
    unit_cost: Decimal = Field(..., description="Average unit cost")
    closing_value: Decimal = Field(..., description="Closing stock value (Quantity * Unit Cost)")
    reorder_level: Optional[int] = Field(None, description="Reorder point level")
    warehouse_location: Optional[str] = Field(None, max_length=100, description="Warehouse/bin location")

class InventoryLedgerReport(BaseModel):
    """Complete Inventory Subsidiary Ledger"""
    as_of_date: datetime = Field(default_factory=datetime.utcnow)
    entries: List[InventoryLedgerEntry] = []
    total_opening_value: Decimal = Field(Decimal('0.00'))
    total_stock_in_value: Decimal = Field(Decimal('0.00'))
    total_stock_out_value: Decimal = Field(Decimal('0.00'))
    total_closing_value: Decimal = Field(Decimal('0.00'))
    item_count: int = Field(0)
    low_stock_count: int = Field(0)

# --- Petty Cash Book ---
class PettyCashEntryBase(BaseModel):
    """Records small, miscellaneous cash expenses - Supporting Record"""
    voucher_number: str = Field(..., max_length=50, description="Petty cash voucher number")
    voucher_date: datetime = Field(default_factory=datetime.utcnow, description="Date of expenditure")
    payee: str = Field(..., max_length=200, description="Person or vendor paid")
    amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(..., description="Amount paid")
    category: Literal['office_supplies', 'postage', 'transportation', 'meals', 'tips', 'miscellaneous'] = Field('miscellaneous', description="Expense category")
    description: str = Field(..., max_length=500, description="Description of expenditure")
    receipt_number: Optional[str] = Field(None, max_length=50, description="Receipt number")
    approved_by: Optional[str] = Field(None, max_length=200, description="Name of person who approved")

    @validator('amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class PettyCashEntryCreate(PettyCashEntryBase):
    pass

class PettyCashEntryInDB(PettyCashEntryBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this entry")
    petty_cash_fund_id: str = Field(..., description="Petty cash fund ID")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    journal_entry_id: Optional[str] = Field(None, description="Linked journal entry ID")

    class Config:
        from_attributes = True

class PettyCashFundBase(BaseModel):
    """Petty Cash Fund record"""
    fund_name: str = Field(..., max_length=100, description="Name of petty cash fund")
    fund_number: str = Field(..., max_length=20, description="Fund identifier number")
    imprest_amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(..., description="Imprest/float amount")
    current_balance: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('0.00'), description="Current balance")
    custodian: str = Field(..., max_length=200, description="Person responsible for fund")
    location: Optional[str] = Field(None, max_length=200, description="Location of fund")

    @validator('imprest_amount', 'current_balance', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class PettyCashFundCreate(PettyCashFundBase):
    pass

class PettyCashFundInDB(PettyCashFundBase):
    id: str = Field(..., example="uuid-string-for-node")
    user_id: str = Field(..., description="ID of the user who owns this fund")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Bank Reconciliation ---
class BankReconciliationEntry(BaseModel):
    """Bank statement line item for reconciliation"""
    transaction_date: datetime = Field(..., description="Date of transaction")
    description: str = Field(..., max_length=500, description="Transaction description")
    debit: Decimal = Field(Decimal('0.00'), description="Debit amount (bank withdrawals)")
    credit: Decimal = Field(Decimal('0.00'), description="Credit amount (bank deposits)")
    running_balance: Decimal = Field(..., description="Running balance after this transaction")
    reference: Optional[str] = Field(None, max_length=100, description="Reference number")
    matched: bool = Field(False, description="Whether this item is matched to company records")
    match_type: Optional[Literal['journal_entry', 'petty_cash', 'sales_receipt', 'payment']] = Field(None, description="Type of matching record")
    matched_entry_id: Optional[str] = Field(None, description="ID of matched entry if any")

class BankReconciliationStatement(BaseModel):
    """Bank Reconciliation Statement - Supporting Record"""
    bank_account_number: str = Field(..., max_length=50, description="Bank account number")
    bank_name: str = Field(..., max_length=200, description="Bank name")
    statement_date: datetime = Field(..., description="Statement date")
    statement_balance: Decimal = Field(..., description="Balance per bank statement")
    book_balance: Decimal = Field(..., description="Balance per company's books")

    # Adjustments to Bank Statement
    deposits_in_transit: List[BankReconciliationEntry] = Field(default_factory=list, description="Deposits not yet received by bank")
    outstanding_checks: List[BankReconciliationEntry] = Field(default_factory=list, description="Checks issued but not yet cleared")

    # Adjustments to Book Balance
    bank_charges: Decimal = Field(Decimal('0.00'), description="Bank service charges not in books")
    interest_earned: Decimal = Field(Decimal('0.00'), description="Interest earned not in books")
    insufficient_funds: Decimal = Field(Decimal('0.00'), description="NSF checks not in books")
    other_adjustments: Decimal = Field(Decimal('0.00'), description="Other adjustments")

    # Results
    adjusted_bank_balance: Decimal = Field(..., description="Adjusted bank balance")
    adjusted_book_balance: Decimal = Field(..., description="Adjusted book balance")
    difference: Decimal = Field(Decimal('0.00'), description="Difference between adjusted balances (should be 0)")

    # Reconciliation items
    bank_entries: List[BankReconciliationEntry] = Field(default_factory=list, description="All entries from bank statement")
    journal_entries: List[BankReconciliationEntry] = Field(default_factory=list, description="All entries from company books")

    is_reconciled: bool = Field(False, description="Whether bank is fully reconciled")
    reconciled_date: Optional[datetime] = Field(None, description="Date reconciliation was completed")
    reconciled_by: Optional[str] = Field(None, max_length=200, description="Person who reconciled")

# --- Error Response Model ---
class ErrorResponse(BaseModel):
    detail: str
    code: Optional[str] = None
    status_code: int = 500
