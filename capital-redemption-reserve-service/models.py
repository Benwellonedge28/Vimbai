"""Pydantic models for Capital Redemption Reserve Service (API contract unchanged)."""

import uuid
from datetime import datetime
from typing import Optional

from pydantic import BaseModel, Field


class RedemptionTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    share_class: str  # preference, ordinary
    shares_redeemed: int
    redemption_price: float
    nominal_value: float
    total_proceeds: float = 0
    redemption_reserve_amount: float = 0  # proceeds - nominal
    redemption_date: datetime
    source_account: str  # proceeds, fresh_issue, bonus_issue
    journal_entry_id: Optional[str] = None
    status: str = "completed"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CRRCreation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    amount: float
    source: str  # share_redemption, capital_reduction, fresh_issue
    description: str
    creation_date: datetime
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CRRUtilization(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    amount: float
    utilization_type: str  # bonus_issue, write_off, transfer_general
    description: str
    utilization_date: datetime
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
