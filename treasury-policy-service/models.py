"""Pydantic models for Treasury Policy Service (API contract unchanged)."""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


class TreasuryPolicy(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    policy_category: str  # liquidity, funding, investment, fx_risk, interest_rate, counterparty
    version: str = "1.0"
    effective_date: datetime
    review_date: Optional[datetime] = None
    approved_by: str = ""
    status: str = "active"  # draft, active, superseded, retired
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class PolicyLimit(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    policy_id: str
    limit_type: str  # exposure, concentration, duration, counterparty
    limit_value: float
    currency: str = "USD"
    warning_threshold: float = 0.8  # 80% of limit
    current_utilization: float = 0.0


class ComplianceCheck(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    policy_id: str
    limit_id: str
    checked_value: float
    limit_value: float
    compliant: bool
    utilization_pct: float
    checked_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    notes: str = ""
