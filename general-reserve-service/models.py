"""Pydantic models for General Reserve Service (API contract unchanged)."""

import uuid
from datetime import datetime
from typing import List, Optional

from pydantic import BaseModel, Field


class GeneralReserve(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    reserve_name: str
    description: str = ""
    current_balance: float = 0
    target_balance: Optional[float] = None
    minimum_balance: float = 0
    funding_source: str = "retained_earnings"  # retained_earnings, share_premium, specific_allocation
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)


class ReserveAllocation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    reserve_id: str
    amount: float
    allocation_date: datetime
    source: str  # retained_earnings, share_premium, profit
    description: str
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ReserveUtilization(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    reserve_id: str
    amount: float
    utilization_date: datetime
    purpose: str  # asset_purchase, debt_repayment, working_capital, bonus_issue
    description: str
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
