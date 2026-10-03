"""Pydantic models for Family & Community Group Service (API contract unchanged)."""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


class GroupMember(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    email: str = ""
    phone: str = ""
    contribution_amount: float = 0.0
    joined_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    active: bool = True


class Contribution(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    group_id: str
    member_id: str
    amount: float
    contribution_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    cycle_number: int
    notes: str = ""


class CommunityGroup(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    contribution_frequency: str = "monthly"  # weekly, biweekly, monthly
    contribution_amount: float
    member_count: int = 0
    current_cycle: int = 1
    total_pool: float = 0.0
    status: str = "active"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class PayoutSchedule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    group_id: str
    member_id: str
    cycle_number: int
    payout_amount: float
    status: str = "scheduled"  # scheduled, paid, missed
    scheduled_date: datetime
    paid_at: Optional[datetime] = None
