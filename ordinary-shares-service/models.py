"""Pydantic models for Ordinary Shares Service"""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


class OrdinaryDividend(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    dividend_type: str  # interim, final, special
    per_share_amount: float
    total_shares: int
    total_dividend: float = 0
    record_date: datetime
    payment_date: Optional[datetime] = None
    status: str = "declared"
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
