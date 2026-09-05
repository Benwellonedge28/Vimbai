"""Pydantic models for Issued Share Capital Service"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


class Shareholder(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    name: str
    address: str
    shareholder_type: str = "individual"
    shares_held: int = 0
    percentage: float = 0


class ShareIssue(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    issue_date: datetime
    share_class: str
    shares_issued: int
    issue_price: float
    total_proceeds: float = 0
    shareholders: List[Dict[str, Any]] = []
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
