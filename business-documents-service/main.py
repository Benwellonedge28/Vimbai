"""
FinAcc Business Documents Service
Comprehensive document management for all business types and transactions.
Documents can be used by any internal service.
"""

import os
import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "business-documents-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8031"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Business Documents Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class DocumentType(str, Enum):
    # Financial Documents
    INVOICE = "invoice"
    CREDIT_NOTE = "credit_note"
    DEBIT_NOTE = "debit_note"
    PROFORMA_INVOICE = "proforma_invoice"
    PURCHASE_ORDER = "purchase_order"
    GOODS_RECEIVED_NOTE = "goods_received_note"
    DELIVERY_NOTE = "delivery_note"

    # Banking Documents
    CHEQUE = "cheque"
    PAYMENT_VOUCHER = "payment_voucher"
    RECEIPT = "receipt"
    BANK_STATEMENT = "bank_statement"
    PETTY_CASH_VOUCHER = "petty_cash_voucher"

    # Partnership Documents
    PARTNERSHIP_AGREEMENT = "partnership_agreement"
    PARTNERSHIP_DEED = "partnership_deed"
    PARTNERSHIP_ADMISSION_DEED = "partnership_admission_deed"
    PARTNERSHIP_RETIREMENT_DEED = "partnership_retirement_deed"
    PARTNERSHIP_DISSOLUTION_DEED = "partnership_dissolution_deed"
    GOODWILL_VALUATION = "goodwill_valuation"
    ASSET_REVALUATION_CERTIFICATE = "asset_revaluation_certificate"

    # Company Documents
    MEMORANDUM_OF_ASSOCIATION = "memorandum_of_association"
    ARTICLES_OF_ASSOCIATION = "articles_of_association"
    SHARE_CERTIFICATE = "share_certificate"
    SHARE_TRANSFER_FORM = "share_transfer_form"
    DIVIDEND_WARRANT = "dividend_warrant"
    DEBENTURE_CERTIFICATE = "debenture_certificate"
    DIRECTORS_REPORT = "directors_report"
    AUDITORS_REPORT = "auditors_report"
    BALANCE_SHEET = "balance_sheet"
    PROFIT_LOSS_ACCOUNT = "profit_loss_account"

    # Asset Documents
    ASSET_REGISTER_ENTRY = "asset_register_entry"
    DEPRECIATION_SCHEDULE = "depreciation_schedule"
    ASSET_DISPOSAL_FORM = "asset_disposal_form"
    ASSET_REVALUATION_REPORT = "asset_revaluation_report"

    # Tax Documents
    TAX_INVOICE = "tax_invoice"
    TAX_CREDIT_NOTE = "tax_credit_note"
    TAX_RETURN = "tax_return"
    WITHHOLDING_TAX_CERTIFICATE = "withholding_tax_certificate"

    # Other Documents
    JOURNAL_VOUCHER = "journal_voucher"
    CONTRAL_VOUCHER = "control_voucher"
   Petty Cash Voucher


class BusinessType(str, Enum):
    SOLE_TRADER = "sole_trader"
    PARTNERSHIP = "partnership"
    LIMITED_COMPANY = "limited_company"
    PUBLIC_COMPANY = "public_company"
    NON_PROFIT = "non_profit"
    COOPERATIVE = "cooperative"


class DocumentStatus(str, Enum):
    DRAFT = "draft"
    PENDING = "pending"
    APPROVED = "approved"
    POSTED = "posted"
    CANCELLED = "cancelled"
    ARCHIVED = "archived"


class Address(BaseModel):
    line1: str
    line2: Optional[str] = None
    city: str
    state: Optional[str] = None
    postal_code: Optional[str] = None
    country: str = "USA"


class PartyDetails(BaseModel):
    id: Optional[str] = None
    name: str
    address: Address
    tax_id: Optional[str] = None
    registration_number: Optional[str] = None
    contact_person: Optional[str] = None
    email: Optional[str] = None
    phone: Optional[str] = None


class LineItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    item_code: str
    description: str
    quantity: float
    unit_price: float
    discount_percent: float = 0
    discount_amount: float = 0
    taxable_amount: float = 0
    tax_rate: float = 0
    tax_amount: float = 0
    line_total: float = 0


class BusinessDocument(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    document_type: DocumentType
    document_number: str
    reference_number: Optional[str] = None
    business_type: BusinessType
    status: DocumentStatus = DocumentStatus.DRAFT

    # Dates
    document_date: datetime
    due_date: Optional[datetime] = None
    posting_date: Optional[datetime] = None

    # Parties
    issuer: PartyDetails
    recipient: PartyDetails
    ship_to: Optional[PartyDetails] = None

    # Financial Details
    currency: str = "USD"
    exchange_rate: float = 1.0
    subtotal: float = 0
    total_discount: float = 0
    total_tax: float = 0
    total_amount: float = 0
    amount_in_words: Optional[str] = None

    # Line Items
    line_items: List[LineItem] = []

    # Additional Info
    notes: Optional[str] = None
    terms_and_conditions: Optional[str] = None
    attachments: List[str] = []

    # Audit
    created_by: Optional[str] = None
    approved_by: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    posted_at: Optional[datetime] = None

    # Linked Documents
    linked_documents: List[str] = []
    reversed_by: Optional[str] = None


class DocumentTemplate(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    document_type: DocumentType
    business_type: BusinessType
    template_content: Dict[str, Any] = {}
    is_active: bool = True
    created_at: datetime = Field(default_factory=datetime.utcnow)


class DocumentSequence(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    document_type: DocumentType
    prefix: str
    current_number: int = 0
    suffix: Optional[str] = None
    padding: int = 5
    is_active: bool = True


# In-memory storage
documents: Dict[str, BusinessDocument] = {}
document_templates: Dict[str, DocumentTemplate] = {}
document_sequences: Dict[str, DocumentSequence] = {}


async def call_audit_service(action: str, resource_type: str, resource_id: str, details: Dict[str, Any]):
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            await client.post(f"{AUDIT_SERVICE_URL}/audit", json={
                "action": action, "resource_type": resource_type, "resource_id": resource_id,
                "details": details, "timestamp": datetime.utcnow().isoformat()
            })
    except Exception as e:
        logger.error("audit_service_call_error", error=str(e))


def calculate_line_totals(items: List[LineItem]) -> tuple[float, float, float, float]:
    """Calculate line item totals."""
    subtotal = 0
    total_discount = 0
    total_tax = 0

    for item in items:
        item.taxable_amount = item.quantity * item.unit_price
        item.discount_amount = item.taxable_amount * (item.discount_percent / 100)
        item.taxable_amount -= item.discount_amount
        item.tax_amount = item.taxable_amount * (item.tax_rate / 100)
        item.line_total = item.taxable_amount + item.tax_amount

        subtotal += item.taxable_amount
        total_discount += item.discount_amount
        total_tax += item.tax_amount

    total = subtotal + total_tax
    return subtotal, total_discount, total_tax, total


def number_to_words(num: float) -> str:
    """Convert number to words."""
    ones = ["", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine",
            "Ten", "Eleven", "Twelve", "Thirteen", "Fourteen", "Fifteen", "Sixteen",
            "Seventeen", "Eighteen", "Nineteen"]
    tens = ["", "", "Twenty", "Thirty", "Forty", "Fifty", "Sixty", "Seventy", "Eighty", "Ninety"]

    if num < 20:
        return ones[int(num)]
    elif num < 100:
        return tens[int(num // 10)] + (" " + ones[int(num % 10)] if num % 10 else "")
    elif num < 1000:
        return ones[int(num // 100)] + " Hundred" + (" " + number_to_words(num % 100) if num % 100 else "")
    elif num < 1000000:
        return number_to_words(num // 1000) + " Thousand" + (" " + number_to_words(num % 1000) if num % 1000 else "")
    else:
        return str(num)


# ============================================================================
# API Endpoints
# ============================================================================

@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy", "timestamp": datetime.utcnow().isoformat()}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Business documents for all transaction types"}


@app.post("/documents", response_model=BusinessDocument, status_code=status.HTTP_201_CREATED)
async def create_document(data: BusinessDocument):
    """Create a new business document."""
    doc_id = str(uuid.uuid4())
    data.id = doc_id
    data.created_at = datetime.utcnow()
    data.updated_at = datetime.utcnow()

    # Calculate totals
    data.subtotal, data.total_discount, data.total_tax, data.total_amount = calculate_line_totals(data.line_items)
    data.amount_in_words = number_to_words(data.total_amount)

    # Generate document number if not provided
    if not data.document_number:
        seq_key = f"{data.business_type.value}_{data.document_type.value}"
        if seq_key in document_sequences:
            seq = document_sequences[seq_key]
            seq.current_number += 1
            data.document_number = f"{seq.prefix}{str(seq.current_number).zfill(seq.padding)}{seq.suffix or ''}"
        else:
            data.document_number = f"DOC-{doc_id[:8]}"

    documents[doc_id] = data

    await call_audit_service("CREATE", "document", doc_id, {"type": data.document_type, "amount": data.total_amount})
    logger.info("document_created", doc_id=doc_id, type=data.document_type)
    return data


@app.get("/documents/{document_id}")
async def get_document(document_id: str):
    """Get document by ID."""
    doc = documents.get(document_id)
    if not doc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Document {document_id} not found")
    return doc


@app.get("/documents")
async def list_documents(
    document_type: Optional[DocumentType] = None,
    business_type: Optional[BusinessType] = None,
    status: Optional[DocumentStatus] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100
):
    """List documents with filters."""
    result = list(documents.values())

    if document_type:
        result = [d for d in result if d.document_type == document_type]
    if business_type:
        result = [d for d in result if d.business_type == business_type]
    if status:
        result = [d for d in result if d.status == status]
    if start_date:
        result = [d for d in result if d.document_date >= start_date]
    if end_date:
        result = [d for d in result if d.document_date <= end_date]

    result.sort(key=lambda x: x.document_date, reverse=True)
    return {"documents": result[:limit], "count": len(result)}


@app.put("/documents/{document_id}")
async def update_document(document_id: str, data: Dict[str, Any]):
    """Update document."""
    doc = documents.get(document_id)
    if not doc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Document {document_id} not found")

    if doc.status == DocumentStatus.POSTED:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Cannot modify posted document")

    for key, value in data.items():
        if hasattr(doc, key) and key not in ["id", "created_at"]:
            setattr(doc, key, value)

    doc.updated_at = datetime.utcnow()

    # Recalculate totals if line items changed
    if "line_items" in data:
        doc.subtotal, doc.total_discount, doc.total_tax, doc.total_amount = calculate_line_totals(doc.line_items)
        doc.amount_in_words = number_to_words(doc.total_amount)

    await call_audit_service("UPDATE", "document", document_id, {"updated_fields": list(data.keys())})
    return doc


@app.post("/documents/{document_id}/approve")
async def approve_document(document_id: str, approved_by: str):
    """Approve a document."""
    doc = documents.get(document_id)
    if not doc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Document {document_id} not found")

    if doc.status != DocumentStatus.DRAFT and doc.status != DocumentStatus.PENDING:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=f"Cannot approve document in {doc.status} status")

    doc.status = DocumentStatus.APPROVED
    doc.approved_by = approved_by
    doc.updated_at = datetime.utcnow()

    await call_audit_service("APPROVE", "document", document_id, {"approved_by": approved_by})
    return doc


@app.post("/documents/{document_id}/post")
async def post_document(document_id: str):
    """Post a document to the accounting system."""
    doc = documents.get(document_id)
    if not doc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Document {document_id} not found")

    if doc.status == DocumentStatus.POSTED:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Document already posted")

    if doc.status != DocumentStatus.APPROVED:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Document must be approved before posting")

    # Create journal entry based on document type
    journal_entries = []

    if doc.document_type == DocumentType.INVOICE:
        journal_entries = [
            {"account": "debtors", "debit": doc.total_amount, "credit": 0, "description": "Accounts Receivable"},
            {"account": "sales", "debit": 0, "credit": doc.subtotal, "description": "Sales Revenue"},
            {"account": "tax", "debit": 0, "credit": doc.total_tax, "description": "Output Tax"},
        ]
    elif doc.document_type == DocumentType.CREDIT_NOTE:
        journal_entries = [
            {"account": "sales", "debit": doc.subtotal, "credit": 0, "description": "Sales Returns"},
            {"account": "tax", "debit": doc.total_tax, "credit": 0, "description": "Output Tax Adjustment"},
            {"account": "debtors", "debit": 0, "credit": doc.total_amount, "description": "Accounts Receivable"},
        ]

    doc.status = DocumentStatus.POSTED
    doc.posted_at = datetime.utcnow()
    doc.updated_at = datetime.utcnow()

    await call_audit_service("POST", "document", document_id, {"journal_entries": journal_entries})
    return {"document": doc, "journal_entries": journal_entries}


@app.post("/documents/{document_id}/reverse")
async def reverse_document(document_id: str, reversal_reason: str):
    """Reverse a posted document."""
    doc = documents.get(document_id)
    if not doc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Document {document_id} not found")

    if doc.status != DocumentStatus.POSTED:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Only posted documents can be reversed")

    # Create reversal document
    reversal = BusinessDocument(
        document_type=DocumentType.CREDIT_NOTE if doc.document_type == DocumentType.INVOICE else DocumentType.INVOICE,
        document_number="",
        business_type=doc.business_type,
        document_date=datetime.utcnow(),
        issuer=doc.issuer,
        recipient=doc.recipient,
        line_items=doc.line_items,
        notes=f"Reversal of {doc.document_number}: {reversal_reason}"
    )

    reversal.reversed_by = document_id
    return reversal


@app.post("/documents/{document_id}/link/{linked_document_id}")
async def link_documents(document_id: str, linked_document_id: str):
    """Link two documents together."""
    doc = documents.get(document_id)
    linked = documents.get(linked_document_id)

    if not doc or not linked:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")

    if document_id not in linked.linked_documents:
        linked.linked_documents.append(document_id)
    if linked_document_id not in doc.linked_documents:
        doc.linked_documents.append(linked_document_id)

    await call_audit_service("LINK", "documents", document_id, {"linked_to": linked_document_id})
    return {"document": doc, "linked_document": linked}


# ============================================================================
# Document Templates
# ============================================================================

@app.post("/templates", response_model=DocumentTemplate, status_code=status.HTTP_201_CREATED)
async def create_template(data: DocumentTemplate):
    """Create a document template."""
    data.id = str(uuid.uuid4())
    data.created_at = datetime.utcnow()
    document_templates[data.id] = data
    return data


@app.get("/templates")
async def list_templates(document_type: Optional[DocumentType] = None, business_type: Optional[BusinessType] = None):
    """List document templates."""
    result = list(document_templates.values())
    if document_type:
        result = [t for t in result if t.document_type == document_type]
    if business_type:
        result = [t for t in result if t.business_type == business_type]
    return {"templates": result, "count": len(result)}


@app.get("/templates/{template_id}")
async def get_template(template_id: str):
    """Get template by ID."""
    template = document_templates.get(template_id)
    if not template:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Template {template_id} not found")
    return template


# ============================================================================
# Document Sequences
# ============================================================================

@app.post("/sequences", response_model=DocumentSequence, status_code=status.HTTP_201_CREATED)
async def create_sequence(data: DocumentSequence):
    """Create document number sequence."""
    data.id = str(uuid.uuid4())
    document_sequences[f"{data.prefix}_{data.document_type.value}"] = data
    return data


@app.get("/sequences")
async def list_sequences():
    """List all sequences."""
    return {"sequences": list(document_sequences.values())}


# ============================================================================
# Document Types Info
# ============================================================================

@app.get("/document-types")
async def get_document_types():
    """Get all document types with descriptions."""
    return {
        "document_types": [
            {"type": dt.value, "name": dt.name.replace("_", " ").title(), "category": _get_document_category(dt)}
            for dt in DocumentType
        ]
    }


def _get_document_category(doc_type: DocumentType) -> str:
    """Get category for document type."""
    financial = [DocumentType.INVOICE, DocumentType.CREDIT_NOTE, DocumentType.DEBIT_NOTE, DocumentType.PROFORMA_INVOICE,
                 DocumentType.PURCHASE_ORDER, DocumentType.GOODS_RECEIVED_NOTE, DocumentType.DELIVERY_NOTE]
    banking = [DocumentType.CHEQUE, DocumentType.PAYMENT_VOUCHER, DocumentType.RECEIPT, DocumentType.BANK_STATEMENT,
               DocumentType.PETTY_CASH_VOUCHER]
    partnership = [DocumentType.PARTNERSHIP_AGREEMENT, DocumentType.PARTNERSHIP_DEED, DocumentType.PARTNERSHIP_ADMISSION_DEED,
                   DocumentType.PARTNERSHIP_RETIREMENT_DEED, DocumentType.PARTNERSHIP_DISSOLUTION_DEED,
                   DocumentType.GOODWILL_VALUATION, DocumentType.ASSET_REVALUATION_CERTIFICATE]
    company = [DocumentType.MEMORANDUM_OF_ASSOCIATION, DocumentType.ARTICLES_OF_ASSOCIATION, DocumentType.SHARE_CERTIFICATE,
               DocumentType.SHARE_TRANSFER_FORM, DocumentType.DIVIDEND_WARRANT, DocumentType.DEBENTURE_CERTIFICATE,
               DocumentType.DIRECTORS_REPORT, DocumentType.AUDITORS_REPORT, DocumentType.BALANCE_SHEET, DocumentType.PROFIT_LOSS_ACCOUNT]
    asset = [DocumentType.ASSET_REGISTER_ENTRY, DocumentType.DEPRECIATION_SCHEDULE, DocumentType.ASSET_DISPOSAL_FORM,
             DocumentType.ASSET_REVALUATION_REPORT]
    tax = [DocumentType.TAX_INVOICE, DocumentType.TAX_CREDIT_NOTE, DocumentType.TAX_RETURN, DocumentType.WITHHOLDING_TAX_CERTIFICATE]

    if doc_type in financial:
        return "Financial"
    elif doc_type in banking:
        return "Banking"
    elif doc_type in partnership:
        return "Partnership"
    elif doc_type in company:
        return "Company"
    elif doc_type in asset:
        return "Asset"
    elif doc_type in tax:
        return "Tax"
    else:
        return "General"


if __name__ == "__main__":
    import uvicorn
    logger.info("starting_business_documents_service", port=PORT)
    uvicorn.run(app, host="0.0.0.0", port=PORT)