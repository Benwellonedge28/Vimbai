"""Vimbai Financial State Machine Service - State machine for financial document lifecycle. Port: 8374"""

import os
import uuid
from collections import defaultdict
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "financial-state-machine-service"
PORT = int(os.getenv("PORT", "8374"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Financial State Machine Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="financial-state-machine-service", instrument_app=app)
except ImportError:
    TRACER = None


class DocumentState(str, Enum):
    DRAFT = "draft"
    PENDING_APPROVAL = "pending_approval"
    APPROVED = "approved"
    POSTED = "posted"
    CANCELLED = "cancelled"
    ARCHIVED = "archived"


TRANSITIONS = {
    DocumentState.DRAFT: [DocumentState.PENDING_APPROVAL, DocumentState.CANCELLED],
    DocumentState.PENDING_APPROVAL: [DocumentState.APPROVED, DocumentState.DRAFT, DocumentState.CANCELLED],
    DocumentState.APPROVED: [DocumentState.POSTED, DocumentState.CANCELLED],
    DocumentState.POSTED: [DocumentState.ARCHIVED],
    DocumentState.CANCELLED: [],
    DocumentState.ARCHIVED: [],
}


class StateTransition(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    document_id: str
    from_state: DocumentState
    to_state: DocumentState
    user_id: str = ""
    notes: str = ""
    timestamp: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class FinancialDocument(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    document_type: str = "invoice"  # invoice, payment, journal_entry, expense
    reference: str = ""
    current_state: DocumentState = DocumentState.DRAFT
    history: List[StateTransition] = []
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


_documents: Dict[str, FinancialDocument] = {}


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/documents", response_model=FinancialDocument)
async def create_document(doc: FinancialDocument):
    _documents[doc.id] = doc
    return doc


@app.get("/documents/{doc_id}")
async def get_document(doc_id: str):
    if doc_id not in _documents:
        raise HTTPException(status_code=404, detail="Document not found")
    return _documents[doc_id]


@app.post("/documents/{doc_id}/transition")
async def transition(doc_id: str, to_state: DocumentState, user_id: str = "", notes: str = ""):
    if doc_id not in _documents:
        raise HTTPException(status_code=404, detail="Document not found")
    doc = _documents[doc_id]
    allowed = TRANSITIONS.get(doc.current_state, [])
    if to_state not in allowed:
        raise HTTPException(
            status_code=400,
            detail=f"Invalid transition: {doc.current_state.value} -> {to_state.value}. Allowed: {[s.value for s in allowed]}",
        )
    transition_record = StateTransition(
        document_id=doc_id, from_state=doc.current_state, to_state=to_state, user_id=user_id, notes=notes
    )
    doc.history.append(transition_record)
    doc.current_state = to_state
    logger.info("state_transition", doc_id=doc_id, from_state=transition_record.from_state, to_state=to_state)
    return {"doc_id": doc_id, "current_state": doc.current_state.value, "history_count": len(doc.history)}


@app.get("/documents/{doc_id}/history")
async def get_history(doc_id: str):
    if doc_id not in _documents:
        raise HTTPException(status_code=404, detail="Document not found")
    return {"doc_id": doc_id, "history": _documents[doc_id].history, "current_state": _documents[doc_id].current_state}


@app.get("/states")
async def get_states():
    return {
        "states": [s.value for s in DocumentState],
        "transitions": {k.value: [v.value for v in vs] for k, vs in TRANSITIONS.items()},
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
