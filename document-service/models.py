"""Pydantic models for Document Service"""

from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


class DocumentType(str, Enum):
    INVOICE = "invoice"
    RECEIPT = "receipt"
    BANK_STATEMENT = "bank_statement"
    CONTRACT = "contract"
    TAX_DOCUMENT = "tax_document"
    EMPLOYEE_RECORD = "employee_record"
    PURCHASE_ORDER = "purchase_order"
    JOURNAL_ENTRY_DOCUMENT = "journal_entry_document"
    OTHER = "other"


class DocumentStatus(str, Enum):
    UPLOADED = "uploaded"
    PROCESSING = "processing"
    OCR_COMPLETED = "ocr_completed"
    INDEXED = "indexed"
    ARCHIVED = "archived"
    DELETED = "deleted"


class DocumentCreate(BaseModel):
    document_type: DocumentType
    title: str = Field(..., min_length=1, max_length=255)
    description: Optional[str] = None
    tags: List[str] = []
    metadata: Optional[Dict[str, Any]] = None
    linked_entity_type: Optional[str] = None  # invoice, journal_entry, etc.
    linked_entity_id: Optional[str] = None


class DocumentInDB(DocumentCreate):
    id: str
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    file_name: str
    file_path: str
    file_size: int
    mime_type: str
    checksum: str
    status: DocumentStatus
    ocr_text: Optional[str] = None
    ocr_confidence: Optional[float] = None
    uploaded_by: str
    created_at: datetime
    updated_at: datetime
    deleted_at: Optional[datetime] = None


class DocumentUpdate(BaseModel):
    title: Optional[str] = None
    description: Optional[str] = None
    tags: Optional[List[str]] = None
    metadata: Optional[Dict[str, Any]] = None
    linked_entity_type: Optional[str] = None
    linked_entity_id: Optional[str] = None


class DocumentSearchRequest(BaseModel):
    query: Optional[str] = None
    document_type: Optional[DocumentType] = None
    tags: Optional[List[str]] = None
    date_from: Optional[datetime] = None
    date_to: Optional[datetime] = None
    linked_entity_type: Optional[str] = None
    linked_entity_id: Optional[str] = None
    limit: int = 50
    offset: int = 0


class OCRResult(BaseModel):
    text: str
    confidence: float
    entities: List[Dict[str, Any]] = []
