"""Pydantic models for Share Premium Service"""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


class SharePremiumEntry(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    entry_type: str  # issue, conversion, reorganization, write_off
    shares_issued: int = 0
    nominal_value: float = 0
    issue_price: float = 0
    premium_amount: float = 0
    share_class: str = "ordinary"
    reference_id: str  # ID of the related share issue
    journal_entry_id: Optional[str] = None
    entry_date: datetime
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class PremiumUtilization(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    amount: float
    utilization_type: str  # bonus_issue, write_off, merger_expenses, legal_costs
    description: str
    journal_entry_id: Optional[str] = None
    utilization_date: datetime
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class PremiumAdjustment(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    adjustment_type: str  # correction, reclassification
    original_amount: float
    adjustment_amount: float
    description: str
    journal_entry_id: Optional[str] = None
    adjustment_date: datetime
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
