"""Pydantic models for the Lease Termination Service.

:LeaseTermination via :OWNS_LEASE_TERMINATION and each calculated settlement as a
:TerminationSettlement node owned by the caller. Both Book-gated.
"""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class LeaseTermination(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    lease_id: str
    termination_date: datetime
    original_end_date: datetime
    remaining_payments: int
    remaining_payment_amount: float
    early_termination_penalty: float = 0.0
    settlement_amount: float = 0.0
    reason: str = ""
    status: str = "pending"  # pending, approved, completed, rejected
    approved_by: str = ""
    created_at: datetime = Field(default_factory=_utcnow)
    completed_at: Optional[datetime] = None


class TerminationSettlement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    termination_id: str
    remaining_lease_payments: float
    early_termination_penalty: float
    asset_return_value: float = 0.0
    net_settlement: float
    created_at: datetime = Field(default_factory=_utcnow)
