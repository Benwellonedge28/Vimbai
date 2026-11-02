"""Pydantic models for the Provision for Doubtful Debts Service.

Debtors under review and the provisions raised from them are nodes now, so the
risk grade and provision a calculation settles on survive restart.

FIXED LATENT BUG: `/provision/create` used to build its record from the summary
dicts `/calculate-provision` returns (debtor_id, name, outstanding, risk,
provision_amount) into a field typed List[DoubtfulDebtEntry], which demands
debtor_name, invoice_number, invoice_date, due_date and amount. Every call raised
a pydantic ValidationError and answered 500, so a provision record could never be
created. The record now carries the full debtor entries.
"""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class ProvisionType(str, Enum):
    GENERAL = "general"
    SPECIFIC = "specific"
    FLAT_RATE = "flat_rate"


class DoubtfulDebtEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    debtor_id: str
    debtor_name: str
    invoice_number: str
    invoice_date: datetime
    due_date: datetime
    amount: float
    outstanding: float
    days_overdue: int = 0
    risk_assessment: str = "medium"  # low, medium, high, doubtful
    provision_percent: float = 0
    provision_amount: float = 0
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=_utcnow)


class DoubtfulDebtProvision(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    period_end: datetime
    provision_type: ProvisionType
    total_debtors: float = 0
    provision_required: float = 0
    provision_opening: float = 0
    provision_adjustment: float = 0
    provision_closing: float = 0
    entries: List[DoubtfulDebtEntry] = []
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=_utcnow)
