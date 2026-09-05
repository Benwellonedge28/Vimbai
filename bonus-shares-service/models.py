"""Pydantic models for Bonus Shares Service"""

import uuid
from datetime import datetime, timezone
from typing import Dict, Optional

from pydantic import BaseModel, Field


class BonusIssue(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    issue_date: datetime
    shares_issued: int
    nominal_value: float
    total_nominal_value: float = 0
    source_reserve: str  # share_premium, retained_earnings, general_reserve
    amount_utilized: float = 0
    shareholder_allocations: Dict[str, int] = {}  # shareholder_id -> shares
    journal_entry_id: Optional[str] = None
    status: str = "approved"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
