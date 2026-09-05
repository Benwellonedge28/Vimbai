# This file may be imported bare (Docker `uvicorn main:app`, bracket mounts), so it
# bootstraps its own package alias before importing sibling modules.
import importlib.util as _ilu
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
_PKG = "document_service"
if _PKG not in _sys.modules or not hasattr(_sys.modules.get(_PKG), "__path__"):
    _spec = _ilu.spec_from_file_location(_PKG, _os.path.join(_HERE, "__init__.py"))
    _pkg = _ilu.module_from_spec(_spec)
    _pkg.__path__ = [_HERE]
    _sys.modules[_PKG] = _pkg

"""
Vimbai Document Management Service
Document storage, OCR processing, and entity linking.

Record-keeping only — file bytes live on the service storage volume; document
metadata persists in Neo4j, user-owned and Book-scoped (X-User-Id / X-Book-ID).
"""

import hashlib
import os
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import List, Optional

import aiofiles
from document_service import crud, models
from document_service.database import Neo4jConnector
from document_service.dependencies import book_id_var, get_db_session, get_user_id
from document_service.exceptions import (
    ConflictError,
    DocumentError,
    NotFoundError,
    ValidationError,
)
from document_service.models import (
    DocumentInDB,
    DocumentSearchRequest,
    DocumentStatus,
    DocumentType,
    DocumentUpdate,
    OCRResult,
)
from dotenv import load_dotenv
from fastapi import (
    BackgroundTasks,
    Depends,
    FastAPI,
    File,
    Form,
    HTTPException,
    Request,
    UploadFile,
    status,
)
from fastapi.responses import JSONResponse, StreamingResponse
from neo4j import AsyncSession
from pydantic import BaseModel, Field

load_dotenv()

SERVICE_NAME = "document-management"
PORT = int(os.getenv("PORT", "8096"))
DOCUMENT_STORAGE_PATH = os.getenv("DOCUMENT_STORAGE_PATH", "/tmp/vimbai_documents")
MAX_FILE_SIZE = int(os.getenv("MAX_FILE_SIZE_MB", "50")) * 1024 * 1024  # 50MB default
ALLOWED_EXTENSIONS = {".pdf", ".jpg", ".jpeg", ".png", ".gif", ".doc", ".docx", ".xls", ".xlsx", ".csv"}

Path(DOCUMENT_STORAGE_PATH).mkdir(parents=True, exist_ok=True)

app = FastAPI(title="Vimbai Document Management Service", version="1.0.0")


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Capture the Book context for the duration of the request."""
    token = book_id_var.set(request.headers.get("x-book-id"))
    try:
        return await call_next(request)
    finally:
        book_id_var.reset(token)


@app.on_event("startup")
async def startup():
    Neo4jConnector.configure(
        os.getenv("NEO4J_URI", "bolt://localhost:7687"),
        os.getenv("NEO4J_USER", "neo4j"),
        os.getenv("NEO4J_PASSWORD", "password"),
    )


@app.on_event("shutdown")
async def shutdown():
    await Neo4jConnector.close_driver()


@app.exception_handler(NotFoundError)
async def not_found_exception_handler(request: Request, exc: NotFoundError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


@app.exception_handler(ConflictError)
async def conflict_exception_handler(request: Request, exc: ConflictError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


@app.exception_handler(ValidationError)
async def validation_exception_handler(request: Request, exc: ValidationError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


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
        ".csv": "text/csv",
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
                {"type": "vendor", "value": "Unknown"},
            ],
        )
    elif file_type == "application/pdf":
        return OCRResult(
            text="[PDF text extraction placeholder]",
            confidence=0.90,
            entities=[{"type": "date", "value": datetime.now(timezone.utc).isoformat()}],
        )
    else:
        return OCRResult(text="", confidence=0.0, entities=[])


# ============================================================================
# API Endpoints
# ============================================================================


@app.get("/")
async def health_check(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    docs = await crud.list_documents(db_session, user_id)
    return {
        "status": "healthy",
        "service": "document-management",
        "total_documents": len([d for d in docs if d.status != DocumentStatus.DELETED]),
        "storage_path": DOCUMENT_STORAGE_PATH,
    }


# --- Document Upload ---


@app.post("/documents", response_model=DocumentInDB, status_code=status.HTTP_201_CREATED)
async def upload_document(
    file: UploadFile = File(...),
    document_type: DocumentType = Form(DocumentType.OTHER),
    title: str = Form(...),
    description: Optional[str] = Form(None),
    tags: Optional[str] = Form(None),  # Comma-separated
    linked_entity_type: Optional[str] = Form(None),
    linked_entity_id: Optional[str] = Form(None),
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Upload a new document"""
    # Validate file
    if not is_allowed_file(file.filename):
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=f"File type not allowed. Allowed: {', '.join(ALLOWED_EXTENSIONS)}",
        )

    # Read file content
    content = await file.read()

    # Check file size
    if len(content) > MAX_FILE_SIZE:
        raise HTTPException(
            status_code=status.HTTP_413_REQUEST_ENTITY_TOO_LARGE,
            detail=f"File too large. Maximum size: {MAX_FILE_SIZE // (1024*1024)}MB",
        )

    # Calculate checksum
    checksum = calculate_checksum(content)

    # Check for duplicate
    existing = await crud.find_active_by_checksum(db_session, user_id, checksum)
    if existing:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Document with same content already exists",
            headers={"X-Existing-Document-ID": existing.id},
        )

    # Generate document ID
    doc_id = str(uuid.uuid4())

    # Save file
    file_ext = get_file_extension(file.filename)
    stored_filename = f"{doc_id}{file_ext}"
    file_path = os.path.join(DOCUMENT_STORAGE_PATH, stored_filename)

    async with aiofiles.open(file_path, "wb") as f:
        await f.write(content)

    # Parse tags
    tag_list = [t.strip() for t in tags.split(",")] if tags else []

    return await crud.create_document(
        db_session,
        user_id,
        file_name=file.filename,
        file_path=file_path,
        file_size=len(content),
        mime_type=get_mime_type(file.filename),
        checksum=checksum,
        document_type=document_type,
        title=title,
        description=description,
        tags=tag_list,
        linked_entity_type=linked_entity_type,
        linked_entity_id=linked_entity_id,
    )


@app.post("/documents/batch", status_code=status.HTTP_201_CREATED)
async def upload_documents_batch(
    files: List[UploadFile] = File(...),
    document_type: DocumentType = Form(DocumentType.OTHER),
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Upload multiple documents"""
    results = []

    for file in files:
        try:
            document = await upload_document(
                file=file,
                document_type=document_type,
                title=file.filename,
                description=None,
                tags=None,
                linked_entity_type=None,
                linked_entity_id=None,
                user_id=user_id,
                db_session=db_session,
            )
            results.append({"filename": file.filename, "status": "uploaded", "document_id": document.id})
        except HTTPException as e:
            results.append({"filename": file.filename, "status": "failed", "error": e.detail})

    return {"total": len(files), "results": results}


# --- Document Retrieval ---


@app.get("/documents/{document_id}", response_model=DocumentInDB)
async def get_document(
    document_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get document metadata"""
    doc = await crud.get_document(db_session, user_id, document_id)
    if not doc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")
    return doc


@app.get("/documents/{document_id}/download")
async def download_document(
    document_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Download document file"""
    doc = await crud.get_document(db_session, user_id, document_id)
    if not doc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")

    if doc.status == DocumentStatus.DELETED:
        raise HTTPException(status_code=status.HTTP_410_GONE, detail="Document has been deleted")

    # Read file
    try:
        async with aiofiles.open(doc.file_path, "rb") as f:
            content = await f.read()
    except FileNotFoundError:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="File not found on disk")

    return StreamingResponse(
        iter([content]),
        media_type=doc.mime_type,
        headers={
            "Content-Disposition": f'attachment; filename="{doc.file_name}"',
            "Content-Length": str(len(content)),
        },
    )


@app.get("/documents/{document_id}/preview")
async def preview_document(
    document_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get document preview (for images)"""
    doc = await crud.get_document(db_session, user_id, document_id)
    if not doc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")

    if not doc.mime_type.startswith("image/"):
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Preview only available for image files")

    try:
        async with aiofiles.open(doc.file_path, "rb") as f:
            content = await f.read()
    except FileNotFoundError:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="File not found on disk")

    return StreamingResponse(iter([content]), media_type=doc.mime_type)


# --- Document Update ---


@app.put("/documents/{document_id}", response_model=DocumentInDB)
async def update_document(
    document_id: str,
    update: DocumentUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    doc = await crud.get_document(db_session, user_id, document_id)
    if not doc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")

    if update.title is not None:
        doc.title = update.title
    if update.description is not None:
        doc.description = update.description
    if update.tags is not None:
        doc.tags = update.tags
    if update.metadata is not None:
        doc.metadata = update.metadata
    if update.linked_entity_type is not None:
        doc.linked_entity_type = update.linked_entity_type
    if update.linked_entity_id is not None:
        doc.linked_entity_id = update.linked_entity_id

    doc.updated_at = datetime.now(timezone.utc)
    await crud.update_document_fields(db_session, user_id, document_id, doc)

    return doc


# --- Document Delete ---


@app.delete("/documents/{document_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_document(
    document_id: str,
    permanent: bool = False,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Delete a document (soft delete by default)"""
    doc = await crud.get_document(db_session, user_id, document_id)
    if not doc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")

    if permanent:
        # Permanent delete: remove file from disk (metadata record removed)
        try:
            os.remove(doc.file_path)
        except FileNotFoundError:
            pass
        await crud.soft_delete_document(db_session, user_id, document_id)
    else:
        # Soft delete
        await crud.soft_delete_document(db_session, user_id, document_id)

    return {"ok": True}


# --- Document Search ---


@app.post("/documents/search")
async def search_documents(
    request: DocumentSearchRequest,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Search documents with filters"""
    results = await crud.list_documents(db_session, user_id)

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
        results = [
            d
            for d in results
            if query_lower in d.title.lower()
            or (d.description and query_lower in d.description.lower())
            or (d.ocr_text and query_lower in d.ocr_text.lower())
        ]

    # Sort by created_at descending
    results.sort(key=lambda x: x.created_at, reverse=True)

    # Paginate
    total = len(results)
    results = results[request.offset : request.offset + request.limit]

    return {"total": total, "limit": request.limit, "offset": request.offset, "documents": results}


# --- OCR Processing ---


@app.post("/documents/{document_id}/ocr")
async def process_ocr(
    document_id: str,
    background_tasks: BackgroundTasks,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Process document with OCR"""
    doc = await crud.get_document(db_session, user_id, document_id)
    if not doc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")

    # Process OCR in background
    background_tasks.add_task(perform_document_ocr, document_id, user_id)

    return {"status": "processing", "document_id": document_id}


async def perform_document_ocr(document_id: str, user_id: str):
    """Background task to perform OCR (best-effort; failures leave status unchanged)"""
    async with Neo4jConnector.get_driver().session() as session:
        doc = await crud.get_document(session, user_id, document_id)
        if not doc:
            return
        try:
            async with aiofiles.open(doc.file_path, "rb") as f:
                content = await f.read()

            ocr_result = await perform_ocr(content, doc.mime_type)
            await crud.save_ocr_results(session, user_id, document_id, ocr_result.text, ocr_result.confidence)
        except Exception:
            pass


# --- Document Listing ---


@app.get("/documents")
async def list_documents(
    document_type: Optional[DocumentType] = None,
    tag: Optional[str] = None,
    limit: int = 50,
    offset: int = 0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List documents with optional filters"""
    results = await crud.list_documents(db_session, user_id)
    results = [d for d in results if d.status != DocumentStatus.DELETED]

    if document_type:
        results = [d for d in results if d.document_type == document_type]

    if tag:
        results = [d for d in results if tag in d.tags]

    results.sort(key=lambda x: x.created_at, reverse=True)
    total = len(results)
    results = results[offset : offset + limit]

    return {"total": total, "documents": results}


# --- Document Statistics ---


@app.get("/statistics")
async def get_statistics(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get document statistics"""
    docs = [d for d in await crud.list_documents(db_session, user_id) if d.status != DocumentStatus.DELETED]

    by_type = {}
    by_status = {}

    for doc in docs:
        type_key = doc.document_type.value
        by_type[type_key] = by_type.get(type_key, 0) + 1

        status_key = doc.status.value
        by_status[status_key] = by_status.get(status_key, 0) + 1

    total_size = sum(d.file_size for d in docs)

    return {
        "total_documents": len(docs),
        "total_size_bytes": total_size,
        "total_size_mb": round(total_size / (1024 * 1024), 2),
        "by_type": by_type,
        "by_status": by_status,
    }


# --- Batch Operations (registered before dynamic routes to avoid path shadowing) ---


class BatchLinkRequest(BaseModel):
    document_ids: List[str]
    entity_type: str
    entity_id: str


@app.post("/documents/batch/link")
async def batch_link_documents(
    request: BatchLinkRequest,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Link multiple documents to an entity"""
    results = []

    for doc_id in request.document_ids:
        try:
            doc = await crud.get_document(db_session, user_id, doc_id)
            if doc:
                doc.linked_entity_type = request.entity_type
                doc.linked_entity_id = request.entity_id
                doc.updated_at = datetime.now(timezone.utc)
                await crud.update_document_fields(db_session, user_id, doc_id, doc)
                results.append({"document_id": doc_id, "status": "linked"})
            else:
                results.append({"document_id": doc_id, "status": "not_found"})
        except Exception as e:
            results.append({"document_id": doc_id, "status": "error", "error": str(e)})

    return {"total": len(request.document_ids), "results": results}


# --- Link Document to Entity ---


@app.post("/documents/{document_id}/link")
async def link_document(
    document_id: str,
    entity_type: str,
    entity_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Link a document to another entity (invoice, journal entry, etc.)"""
    doc = await crud.get_document(db_session, user_id, document_id)
    if not doc:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Document not found")

    doc.linked_entity_type = entity_type
    doc.linked_entity_id = entity_id
    doc.updated_at = datetime.now(timezone.utc)
    await crud.update_document_fields(db_session, user_id, document_id, doc)

    return {"status": "linked", "document_id": document_id, "entity_type": entity_type, "entity_id": entity_id}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
