"""
Vimbai Document Management Service
Centralized document storage, retrieval, and management for financial documents
"""

from fastapi import FastAPI, HTTPException, UploadFile, File, BackgroundTasks, Depends, status
from fastapi.responses import JSONResponse, StreamingResponse
from pydantic import BaseModel, Field
from typing import List, Optional, Dict, Any
from datetime import datetime, timezone
from enum import Enum
import asyncio
import json
import uuid
import os
import hashlib
import aiofiles
from pathlib import Path
from dotenv import load_dotenv

load_dotenv()

app = FastAPI(
    title="Vimbai Document Management Service",
    description="Centralized document storage, OCR processing, and audit trail for financial documents",
    version="1.0.0",
)

# ============================================================================
# Configuration
# ============================================================================

DOCUMENT_STORAGE_PATH = os.getenv("DOCUMENT_STORAGE_PATH", "/tmp/vimbai_documents")
MAX_FILE_SIZE = int(os.getenv("MAX_FILE_SIZE_MB", "50")) * 1024 * 1024  # 50MB default
ALLOWED_EXTENSIONS = {".pdf", ".jpg", ".jpeg", ".png", ".gif", ".doc", ".docx", ".xls", ".xlsx", ".csv"}

# Create storage directory
Path(DOCUMENT_STORAGE_PATH).mkdir(parents=True, exist_ok=True)

# ============================================================================
# Models
# ============================================================================

class DocumentType(str, Enum):
    INVOICE = "invoice"
    RECEIPT = "receipt"
    CONTRACT = "contract"
    STATEMENT = "statement"
    REPORT = "report"
    TAX_DOCUMENT = "tax_document"
    IDENTITY_DOCUMENT = "identity_document"
    BANK_STATEMENT = "bank_statement"
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

# ============================================================================
# In-Memory Storage (Use Neo4j/S3 in production)
# ============================================================================

documents: Dict[str, DocumentInDB] = {}
document_index: Dict[str, List[str]] = {}  # tag -> document_ids

# ============================================================================
# Helper Functions
# ============================================================================

def calculate_checksum(content: bytes) -> str:
    """Calculate SHA-256 checksum of file content"""
    return hashlib.sha256(content).hexdigest()

def get_file_extension(file_name: str) -> str:
    """Get lowercase file extension"""
    return Path(file_name).suffix.lower()

def is_allowed_file(file_name: str) -> bool:
    """Check if file extension is allowed"""
    return get_file_extension(file_name) in ALLOWED_EXTENSIONS

def get_mime_type(file_name: str) -> str:
    """Get MIME type based on file extension"""
    mime_types = {
        ".pdf": "application/pdf",
        ".jpg": "image/jpeg",
        ".jpeg": "image/jpeg",
        ".png": "image/png",
        ".gif": "image/gif",
        ".doc": "application/msword",
        ".docx": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ".xls": "application/vnd.ms-excel",
        ".xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ".csv": "text/csv"
    }
    return mime_types.get(get_file_extension(file_name), "application/octet-stream")

async def perform_ocr(content: bytes, file_type: str) -> OCRResult:
    """Simulate OCR processing (use actual OCR service in production)"""
    # In production, integrate with Tesseract, Google Vision, AWS Textract, etc.
    # For now, simulate OCR result

    if file_type.startswith("image/"):
        return OCRResult(
            text="[OCR placeholder - integrate with actual OCR service]",
            confidence=0.85,
            entities=[
                {"type": "date", "value": datetime.now(timezone.utc).isoformat()},
                {"type": "amount", "value": "0.00"},
                {"type": "vendor", "value": "Unknown"}
            ]
        )
    elif file_type == "application/pdf":
        return OCRResult(
            text="[PDF text extraction placeholder]",
            confidence=0.90,
            entities=[
                {"type": "date", "value": datetime.now(timezone.utc).isoformat()}
            ]
        )
    else:
        return OCRResult(text="", confidence=0.0, entities=[])

async def index_document_content(document: DocumentInDB, ocr_result: OCRResult):
    """Index document for full-text search"""
    # Build search index
    searchable_text = f"{document.title} {document.description or ''} {ocr_result.text}"
    searchable_text = searchable_text.lower()

    # Index by words
    words = searchable_text.split()
    for word in words:
        if len(word) > 2:  # Skip very short words
            if word not in document_index:
                document_index[word] = []
            if document.id not in document_index[word]:
                document_index[word].append(document.id)

# ============================================================================
# API Endpoints
# ============================================================================

@app.get("/")
async def health_check():
    return {
        "status": "healthy",
        "service": "document-management",
        "total_documents": len(documents),
        "storage_path": DOCUMENT_STORAGE_PATH
    }

# --- Document Upload ---
@app.post("/documents", response_model=DocumentInDB, status_code=status.HTTP_201_CREATED)
async def upload_document(
    file: UploadFile = File(...),
    document_type: DocumentType = DocumentType.OTHER,
    title: str = Form(...),
    description: Optional[str] = Form(None),
    tags: Optional[str] = Form(None),  # Comma-separated
    linked_entity_type: Optional[str] = Form(None),
    linked_entity_id: Optional[str] = Form(None),
    user_id: str = "system"
):
    """Upload a new document"""
    # Validate file
    if not is_allowed_file(file.filename):
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=f"File type not allowed. Allowed: {', '.join(ALLOWED_EXTENSIONS)}"
        )

    # Read file content
    content = await file.read()

    # Check file size
    if len(content) > MAX_FILE_SIZE:
        raise HTTPException(
            status_code=status.HTTP_413_REQUEST_ENTITY_TOO_LARGE,
            detail=f"File too large. Maximum size: {MAX_FILE_SIZE // (1024*1024)}MB"
        )

    # Calculate checksum
    checksum = calculate_checksum(content)

    # Check for duplicate
    for doc in documents.values():
        if doc.checksum == checksum and doc.status != DocumentStatus.DELETED:
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Document with same content already exists",
                headers={"X-Existing-Document-ID": doc.id}
            )

    # Generate document ID
    doc_id = str(uuid.uuid4())

    # Save file
    file_ext = get_file_extension(file.filename)
    stored_filename = f"{doc_id}{file_ext}"
    file_path = os.path.join(DOCUMENT_STORAGE_PATH, stored_filename)

    async with aiofiles.open(file_path, 'wb') as f:
        await f.write(content)

    # Parse tags
    tag_list = [t.strip() for t in tags.split(',')] if tags else []

    # Create document record
    now = datetime.now(timezone.utc)
    document = DocumentInDB(
        id=doc_id,
        file_name=file.filename,
        file_path=file_path,
        file_size=len(content),
        mime_type=get_mime_type(file.filename),
        checksum=checksum,
        document_type=document_type,
        title=title,
        description=description,
        tags=tag_list,
        metadata={},
        status=DocumentStatus.UPLOADED,
        linked_entity_type=linked_entity_type,
        linked_entity_id=linked_entity_id,
        uploaded_by=user_id,
        created_at=now,
        updated_at=now
    )

    documents[doc_id] = document

    # Index tags
    for tag in tag_list:
        if tag not in document_index:
            document_index[tag] = []
        document_index[tag].append(doc_id)

    return document

@app.post("/documents/batch", status_code=status.HTTP_201_CREATED)
async def upload_documents_batch(
    files: List[UploadFile] = File(...),
    document_type: DocumentType = DocumentType.OTHER,
    user_id: str = "system"
):
    """Upload multiple documents"""
    results = []

    for file in files:
        try:
            document = await upload_document(
                file=file,
                document_type=document_type,
                title=file.filename,
                user_id=user_id
            )
            results.append({
                "filename": file.filename,
                "status": "uploaded",
                "document_id": document.id
            })
        except HTTPException as e:
            results.append({
                "filename": file.filename,
                "status": "failed",
                "error": e.detail
            })

    return {"total": len(files), "results": results}

# --- Document Retrieval ---
@app.get("/documents/{document_id}", response_model=DocumentInDB)
async def get_document(document_id: str):
    if document_id not in documents:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")

    doc = documents[document_id]
    if doc.status == DocumentStatus.DELETED:
        raise HTTPException(status_code=status.HTTP_410_GONE, detail="Document has been deleted")

    return doc

@app.get("/documents/{document_id}/download")
async def download_document(document_id: str):
    """Download document file"""
    if document_id not in documents:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")

    doc = documents[document_id]
    if doc.status == DocumentStatus.DELETED:
        raise HTTPException(status_code=status.HTTP_410_GONE, detail="Document has been deleted")

    # Read file
    try:
        async with aiofiles.open(doc.file_path, 'rb') as f:
            content = await f.read()
    except FileNotFoundError:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="File not found on disk")

    return StreamingResponse(
        iter([content]),
        media_type=doc.mime_type,
        headers={
            "Content-Disposition": f'attachment; filename="{doc.file_name}"',
            "Content-Length": str(len(content))
        }
    )

@app.get("/documents/{document_id}/preview")
async def preview_document(document_id: str):
    """Get document preview (for images)"""
    if document_id not in documents:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")

    doc = documents[document_id]

    if not doc.mime_type.startswith("image/"):
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Preview only available for image files"
        )

    try:
        async with aiofiles.open(doc.file_path, 'rb') as f:
            content = await f.read()
    except FileNotFoundError:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="File not found on disk")

    return StreamingResponse(
        iter([content]),
        media_type=doc.mime_type
    )

# --- Document Update ---
@app.put("/documents/{document_id}", response_model=DocumentInDB)
async def update_document(
    document_id: str,
    update: DocumentUpdate
):
    if document_id not in documents:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")

    doc = documents[document_id]

    if update.title is not None:
        doc.title = update.title
    if update.description is not None:
        doc.description = update.description
    if update.tags is not None:
        # Update tag index
        for tag in doc.tags:
            if tag in document_index and document_id in document_index[tag]:
                document_index[tag].remove(document_id)

        doc.tags = update.tags

        for tag in update.tags:
            if tag not in document_index:
                document_index[tag] = []
            if document_id not in document_index[tag]:
                document_index[tag].append(document_id)

    if update.metadata is not None:
        doc.metadata = update.metadata
    if update.linked_entity_type is not None:
        doc.linked_entity_type = update.linked_entity_type
    if update.linked_entity_id is not None:
        doc.linked_entity_id = update.linked_entity_id

    doc.updated_at = datetime.now(timezone.utc)

    return doc

# --- Document Delete ---
@app.delete("/documents/{document_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_document(document_id: str, permanent: bool = False):
    """Delete a document (soft delete by default)"""
    if document_id not in documents:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")

    doc = documents[document_id]

    if permanent:
        # Permanent delete
        try:
            os.remove(doc.file_path)
        except FileNotFoundError:
            pass

        # Remove from index
        for tag in doc.tags:
            if tag in document_index and document_id in document_index[tag]:
                document_index[tag].remove(document_id)

        del documents[document_id]
    else:
        # Soft delete
        doc.status = DocumentStatus.DELETED
        doc.deleted_at = datetime.now(timezone.utc)

    return {"ok": True}

# --- Document Search ---
@app.post("/documents/search")
async def search_documents(request: DocumentSearchRequest):
    """Search documents with filters"""
    results = list(documents.values())

    # Filter by status (exclude deleted by default)
    results = [d for d in results if d.status != DocumentStatus.DELETED]

    # Filter by type
    if request.document_type:
        results = [d for d in results if d.document_type == request.document_type]

    # Filter by tags
    if request.tags:
        results = [d for d in results if any(tag in d.tags for tag in request.tags)]

    # Filter by date range
    if request.date_from:
        results = [d for d in results if d.created_at >= request.date_from]
    if request.date_to:
        results = [d for d in results if d.created_at <= request.date_to]

    # Filter by linked entity
    if request.linked_entity_type:
        results = [d for d in results if d.linked_entity_type == request.linked_entity_type]
    if request.linked_entity_id:
        results = [d for d in results if d.linked_entity_id == request.linked_entity_id]

    # Full-text search
    if request.query:
        query_lower = request.query.lower()
        results = [d for d in results if
            query_lower in d.title.lower() or
            (d.description and query_lower in d.description.lower()) or
            (d.ocr_text and query_lower in d.ocr_text.lower())
        ]

    # Sort by created_at descending
    results.sort(key=lambda x: x.created_at, reverse=True)

    # Paginate
    total = len(results)
    results = results[request.offset:request.offset + request.limit]

    return {
        "total": total,
        "limit": request.limit,
        "offset": request.offset,
        "documents": results
    }

# --- OCR Processing ---
@app.post("/documents/{document_id}/ocr")
async def process_ocr(document_id: str, background_tasks: BackgroundTasks):
    """Process document with OCR"""
    if document_id not in documents:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")

    doc = documents[document_id]
    doc.status = DocumentStatus.PROCESSING
    doc.updated_at = datetime.now(timezone.utc)

    # Process OCR in background
    background_tasks.add_task(perform_document_ocr, document_id)

    return {"status": "processing", "document_id": document_id}

async def perform_document_ocr(document_id: str):
    """Background task to perform OCR"""
    doc = documents[document_id]

    try:
        # Read file content
        async with aiofiles.open(doc.file_path, 'rb') as f:
            content = await f.read()

        # Perform OCR
        ocr_result = await perform_ocr(content, doc.mime_type)

        # Update document
        doc.ocr_text = ocr_result.text
        doc.ocr_confidence = ocr_result.confidence
        doc.status = DocumentStatus.OCR_COMPLETED

        # Index content
        await index_document_content(doc, ocr_result)

    except Exception as e:
        doc.status = DocumentStatus.UPLOADED
        doc.metadata = {"ocr_error": str(e)}

    doc.updated_at = datetime.now(timezone.utc)

# --- Document Listing ---
@app.get("/documents")
async def list_documents(
    document_type: Optional[DocumentType] = None,
    tag: Optional[str] = None,
    limit: int = 50,
    offset: int = 0
):
    """List documents with optional filters"""
    results = [d for d in documents.values() if d.status != DocumentStatus.DELETED]

    if document_type:
        results = [d for d in results if d.document_type == document_type]

    if tag:
        results = [d for d in results if tag in d.tags]

    results.sort(key=lambda x: x.created_at, reverse=True)
    total = len(results)
    results = results[offset:offset + limit]

    return {"total": total, "documents": results}

# --- Document Statistics ---
@app.get("/statistics")
async def get_statistics():
    """Get document statistics"""
    total = len([d for d in documents.values() if d.status != DocumentStatus.DELETED])
    by_type = {}
    by_status = {}

    for doc in documents.values():
        if doc.status == DocumentStatus.DELETED:
            continue

        type_key = doc.document_type.value
        by_type[type_key] = by_type.get(type_key, 0) + 1

        status_key = doc.status.value
        by_status[status_key] = by_status.get(status_key, 0) + 1

    total_size = sum(d.file_size for d in documents.values() if d.status != DocumentStatus.DELETED)

    return {
        "total_documents": total,
        "total_size_bytes": total_size,
        "total_size_mb": round(total_size / (1024 * 1024), 2),
        "by_type": by_type,
        "by_status": by_status
    }

# --- Link Document to Entity ---
@app.post("/documents/{document_id}/link")
async def link_document(
    document_id: str,
    entity_type: str,
    entity_id: str
):
    """Link a document to another entity (invoice, journal entry, etc.)"""
    if document_id not in documents:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")

    doc = documents[document_id]
    doc.linked_entity_type = entity_type
    doc.linked_entity_id = entity_id
    doc.updated_at = datetime.now(timezone.utc)

    return {"status": "linked", "document_id": document_id, "entity_type": entity_type, "entity_id": entity_id}


# --- Batch Operations ---
@app.post("/documents/batch/link")
async def batch_link_documents(
    document_ids: List[str],
    entity_type: str,
    entity_id: str
):
    """Link multiple documents to an entity"""
    results = []

    for doc_id in document_ids:
        try:
            if doc_id in documents:
                doc = documents[doc_id]
                doc.linked_entity_type = entity_type
                doc.linked_entity_id = entity_id
                doc.updated_at = datetime.now(timezone.utc)
                results.append({"document_id": doc_id, "status": "linked"})
            else:
                results.append({"document_id": doc_id, "status": "not_found"})
        except Exception as e:
            results.append({"document_id": doc_id, "status": "error", "error": str(e)})

    return {"total": len(document_ids), "results": results}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8096)