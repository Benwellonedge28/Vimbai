"""Pydantic models for Preference Shares Service"""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


class PreferenceShareClass(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    company_id: str
    nominal_value: float
    issue_price: float
    fixed_dividend_rate: float  # Annual percentage rate
    dividend_type: str  # cumulative, non_cumulative
    participation_rights: str  # full, limited, none
    liquidation_priority: int  # 1 = highest priority
    redemption_terms: Optional[str] = None
    conversion_terms: Optional[str] = None
    shares_issued: int = 0
    shares_outstanding: int = 0


class PreferenceDividend(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    share_class_id: str
    company_id: str
    dividend_type: str  # fixed, participating_surplus
    per_share_amount: float
    total_shares: int
    total_dividend: float = 0
    preference_arears: float = 0  # For cumulative shares
    record_date: datetime
    payment_date: Optional[datetime] = None
    journal_entry_id: Optional[str] = None
    status: str = "declared"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class RedemptionEntry(BaseModel):
    book_id: Optional[str] = None
    user_id: Optional[str] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    share_class_id: str
    shares_redeemed: int
    redemption_price: float
    total_proceeds: float = 0
    redemption_date: datetime
    journal_entry_id: Optional[str] = None
    status: str = "completed"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
