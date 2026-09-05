"""
Document Service CRUD Operations

Neo4j-backed persistence for document metadata (file bytes live on the
service's storage volume). Records are user-owned and Book-scoped; every
read applies `WHERE ($book_id IS NULL OR x.book_id = $book_id)`.
tags / metadata are persisted as JSON properties.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from document_service.dependencies import book_id_var
from document_service.exceptions import ConflictError, NotFoundError, ValidationError
from document_service.models import DocumentInDB, DocumentStatus, DocumentType
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _dt(value) -> Optional[datetime]:
    if value is None:
        return None
    if isinstance(value, str):
        return datetime.fromisoformat(value)
    return datetime.fromisoformat(value.iso_format())


def _json_list(value) -> list:
    if not value:
        return []
    if isinstance(value, str):
        try:
            return json.loads(value)
        except (TypeError, ValueError):
            return []
    return list(value)


def _json_dict(value) -> Dict[str, Any]:
    if not value:
        return {}
    if isinstance(value, str):
        try:
            return json.loads(value) or {}
        except (TypeError, ValueError):
            return {}
    return dict(value)


def _doc_from_node(n: Dict[str, Any], user_id: str) -> DocumentInDB:
    return DocumentInDB(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        file_name=n["file_name"],
        file_path=n["file_path"],
        file_size=int(n.get("file_size", 0)),
        mime_type=n.get("mime_type", ""),
        checksum=n["checksum"],
        document_type=DocumentType(n["document_type"]),
        title=n["title"],
        description=n.get("description"),
        tags=_json_list(n.get("tags")),
        metadata=_json_dict(n.get("metadata")),
        status=DocumentStatus(n.get("status", "uploaded")),
        ocr_text=n.get("ocr_text"),
        ocr_confidence=n.get("ocr_confidence"),
        linked_entity_type=n.get("linked_entity_type"),
        linked_entity_id=n.get("linked_entity_id"),
        uploaded_by=n.get("uploaded_by", user_id),
        created_at=_dt(n.get("created_at")),
        updated_at=_dt(n.get("updated_at")),
        deleted_at=_dt(n.get("deleted_at")),
    )


# ============================================================================
# Create / Read
# ============================================================================


async def create_document(
    session: AsyncSession,
    user_id: str,
    *,
    file_name: str,
    file_path: str,
    file_size: int,
    mime_type: str,
    checksum: str,
    document_type: DocumentType,
    title: str,
    description: Optional[str],
    tags: List[str],
    linked_entity_type: Optional[str],
    linked_entity_id: Optional[str],
) -> DocumentInDB:
    doc_id = str(uuid.uuid4())
    now = _now()

    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:DocumentRecord {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        file_name: $file_name,
        file_path: $file_path,
        file_size: toInteger($file_size),
        mime_type: $mime_type,
        checksum: $checksum,
        document_type: $document_type,
        title: $title,
        description: $description,
        tags: $tags,
        metadata: $metadata,
        status: $status,
        ocr_text: $ocr_text,
        ocr_confidence: toFloat($ocr_confidence),
        linked_entity_type: $linked_entity_type,
        linked_entity_id: $linked_entity_id,
        uploaded_by: $uploaded_by,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at),
        deleted_at: $deleted_at
    }})
    CREATE (u)-[:OWNS_DOCUMENT]->(x)
    RETURN x
    """
    params = {
        "id": doc_id,
        "user_id": user_id,
        "file_name": file_name,
        "file_path": file_path,
        "file_size": file_size,
        "mime_type": mime_type,
        "checksum": checksum,
        "document_type": document_type.value,
        "title": title,
        "description": description,
        "tags": json.dumps(tags),
        "metadata": json.dumps({}),
        "status": DocumentStatus.UPLOADED.value,
        "ocr_text": None,
        "ocr_confidence": 0.0,
        "linked_entity_type": linked_entity_type,
        "linked_entity_id": linked_entity_id,
        "uploaded_by": user_id,
        "created_at": now.isoformat(),
        "updated_at": now.isoformat(),
        "deleted_at": None,
    }
    result = await _run(session, query, params)
    record = await result.single()
    return _doc_from_node(record["x"], user_id)


async def get_document(session: AsyncSession, user_id: str, document_id: str) -> Optional[DocumentInDB]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DOCUMENT]->(x:DocumentRecord {{id: $id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, id=document_id, user_id=user_id)
    record = await result.single()
    return _doc_from_node(record["x"], user_id) if record else None


async def find_active_by_checksum(session: AsyncSession, user_id: str, checksum: str) -> Optional[DocumentInDB]:
    """Find a non-deleted document with the same content checksum."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DOCUMENT]->(x:DocumentRecord {{checksum: $checksum}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, checksum=checksum, user_id=user_id)
    docs = [_doc_from_node(r["x"], user_id) async for r in result]
    return next((d for d in docs if d.status != DocumentStatus.DELETED), None)


async def list_documents(session: AsyncSession, user_id: str) -> List[DocumentInDB]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DOCUMENT]->(x:DocumentRecord)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_doc_from_node(r["x"], user_id) async for r in result]


# ============================================================================
# Update / Delete
# ============================================================================


async def update_document_fields(session: AsyncSession, user_id: str, document_id: str, document: DocumentInDB) -> None:
    """Persist field changes (title/description/tags/metadata/links) + updated_at."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DOCUMENT]->(x:DocumentRecord {{id: $id}})
    {BOOK_FILTER}
    SET x.title = $title,
        x.description = $description,
        x.tags = $tags,
        x.metadata = $metadata,
        x.linked_entity_type = $linked_entity_type,
        x.linked_entity_id = $linked_entity_id,
        x.updated_at = datetime($updated_at)
    RETURN x
    """
    params = {
        "id": document_id,
        "user_id": user_id,
        "title": document.title,
        "description": document.description,
        "tags": json.dumps(document.tags),
        "metadata": json.dumps(document.metadata or {}),
        "linked_entity_type": document.linked_entity_type,
        "linked_entity_id": document.linked_entity_id,
        "updated_at": document.updated_at.isoformat(),
    }
    await _run(session, query, params)


async def soft_delete_document(session: AsyncSession, user_id: str, document_id: str) -> None:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DOCUMENT]->(x:DocumentRecord {{id: $id}})
    {BOOK_FILTER}
    SET x.status = $status,
        x.deleted_at = datetime($deleted_at),
        x.updated_at = datetime($updated_at)
    RETURN x
    """
    now = _now()
    await _run(
        session,
        query,
        id=document_id,
        user_id=user_id,
        status=DocumentStatus.DELETED.value,
        deleted_at=now.isoformat(),
        updated_at=now.isoformat(),
    )


async def save_ocr_results(session: AsyncSession, user_id: str, document_id: str, text: str, confidence: float) -> None:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DOCUMENT]->(x:DocumentRecord {{id: $id}})
    {BOOK_FILTER}
    SET x.ocr_text = $ocr_text,
        x.ocr_confidence = toFloat($ocr_confidence),
        x.status = $status,
        x.updated_at = datetime($updated_at)
    RETURN x
    """
    now = _now()
    await _run(
        session,
        query,
        id=document_id,
        user_id=user_id,
        ocr_text=text,
        ocr_confidence=float(confidence),
        status=DocumentStatus.OCR_COMPLETED.value,
        updated_at=now.isoformat(),
    )
