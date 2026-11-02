"""Pydantic models for the Bad Debts Recovery Service.

A written-off debt is a node, and every recovery booked against it is its own
node, so the trail survives restarts instead of living in two lists.
"""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Optional

from pydantic import BaseModel, Field


class RecoveryStatus(str, Enum):
    PENDING = "pending"
    PARTIAL = "partial"
    COMPLETED = "completed"
    UNRECOVERABLE = "unrecoverable"


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class RecoveryEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    original_debt_id: str
    debtor_name: str
    original_amount: float
    written_off_amount: float
    recovered_amount: float = 0
    recovery_date: Optional[datetime] = None
    status: RecoveryStatus = RecoveryStatus.PENDING
    payment_method: Optional[str] = None
    reference: Optional[str] = None
    notes: Optional[str] = None
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=_utcnow)


class RecoveryJournal(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    recovery_id: str
    amount: float
    date: datetime
    description: str
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=_utcnow)
