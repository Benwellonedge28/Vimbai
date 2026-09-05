"""Pydantic models for Authorized Share Capital Service"""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


class ShareClass(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str  # ordinary, preference, founder, treasury
    authorized_shares: int
    issued_shares: int = 0
    par_value: float = 0.0
    voting_rights: str = "ordinary"  # ordinary, preferential, none
    dividend_rate: float = 0.0  # for preference shares
    rights: str = ""  # description of class rights
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ShareIssuance(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    share_class_id: str
    number_of_shares: int
    issue_price: float
    total_proceeds: float = 0.0
    issue_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    issued_to: str = ""
    notes: str = ""


class ShareBuyback(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    share_class_id: str
    number_of_shares: int
    buyback_price: float
    total_cost: float = 0.0
    buyback_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    notes: str = ""
