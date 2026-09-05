"""Pydantic models for Share Redemption Service"""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


class ShareRedemption(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    share_class: str  # preference, ordinary
    shares_redeemed: int
    nominal_value: float
    redemption_price: float
    total_redemption_value: float = 0
    redemption_date: datetime
    redemption_method: str  # proceeds, fresh_issue, existing_assets, combination
    authority_date: datetime  # When redemption was authorized
    statutory_declaration_date: Optional[datetime] = None
    journal_entry_id: Optional[str] = None
    status: str = "pending"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class FreshIssueForRedemption(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    redemption_id: str
    shares_issued: int
    issue_price: float
    nominal_value: float
    total_proceeds: float = 0
    issue_date: datetime
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CRRRequirement(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    redemption_id: str
    nominal_value_of_shares: float
    proceeds_used: float
    fresh_issue_proceeds: float
    minimum_crr_required: float = 0
    crr_created: float = 0
    source_of_crr: str
    compliance_status: str = "pending"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
