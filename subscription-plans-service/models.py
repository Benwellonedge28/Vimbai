"""Pydantic models for the Subscription Plans Service."""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


class BillingCycle(str, Enum):
    MONTHLY = "monthly"
    QUARTERLY = "quarterly"
    ANNUAL = "annual"


class PlanTier(str, Enum):
    FREE = "free"
    BASIC = "basic"
    PROFESSIONAL = "professional"
    ENTERPRISE = "enterprise"


class Plan(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    tier: PlanTier
    name: str
    price_monthly: float
    features: List[str] = []
    max_users: int = 5
    max_companies: int = 1
    api_calls_per_month: int = 1000


class Subscription(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    plan_id: str
    tier: PlanTier
    billing_cycle: BillingCycle = BillingCycle.MONTHLY
    start_date: str = Field(default_factory=lambda: datetime.now(timezone.utc).strftime("%Y-%m-%d"))
    status: str = "active"  # active, cancelled, suspended, past_due
    current_period_end: str = ""


class UpgradeRequest(BaseModel):
    company_id: str
    current_plan: PlanTier
    target_plan: PlanTier
    current_period_end: str
    prorate: bool = True


class UpgradeResult(BaseModel):
    company_id: str
    current_plan: str
    target_plan: str
    proration_amount: float
    effective_date: str
    new_billing_amount: float
    cycle: str
