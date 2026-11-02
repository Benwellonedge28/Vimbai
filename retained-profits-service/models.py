"""Pydantic models for the Retained Profits Service.

Appropriations, interim dividends and prior year adjustments are nodes, so the
statement of what happened to last year's profit survives restart rather than
living in three lists.
"""

import uuid
from datetime import datetime, timezone
from typing import Dict, Optional

from pydantic import BaseModel, Field


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class ProfitAppropriation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    financial_year: str
    period_start: datetime
    period_end: datetime
    net_profit: float
    appropriations: Dict[str, float] = {}
    total_appropriations: float = 0
    balance_carried_forward: float = 0
    opening_retained_earnings: float = 0
    closing_retained_earnings: float = 0
    journal_entry_id: Optional[str] = None
    status: str = "draft"
    created_at: datetime = Field(default_factory=_utcnow)


class InterimDividend(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    amount: float
    per_share_amount: float
    total_shares: int
    declaration_date: datetime
    payment_date: Optional[datetime] = None
    journal_entry_id: Optional[str] = None
    status: str = "declared"
    created_at: datetime = Field(default_factory=_utcnow)


class PriorYearAdjustment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    adjustment_type: str
    description: str
    amount: float
    affected_year: str
    journal_entry_id: Optional[str] = None
    adjustment_date: datetime
    created_at: datetime = Field(default_factory=_utcnow)
