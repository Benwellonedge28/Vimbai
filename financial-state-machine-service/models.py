"""Pydantic models for the Financial State Machine Service."""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import List

from pydantic import BaseModel, Field


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
