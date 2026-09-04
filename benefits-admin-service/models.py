"""Pydantic models for Benefits Administration Service"""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field

VALID_PLAN_TYPES = ["pension", "medical", "dental", "life_insurance", "leave"]
VALID_LEAVE_TYPES = ["annual", "sick", "maternity", "compassionate"]


class BenefitPlan(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    plan_type: str  # pension, medical, dental, life_insurance, leave
    description: str = ""
    employer_contribution_pct: float = 0.0
    employee_contribution_pct: float = 0.0
    eligibility_months: int = 0
    status: str = "active"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class BenefitEnrollment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    employee_id: str
    plan_id: str
    enrollment_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    status: str = "active"  # active, opted_out, terminated
    beneficiary: str = ""


class LeaveAccrual(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    employee_id: str
    leave_type: str  # annual, sick, maternity, compassionate
    period: str  # YYYY-MM
    accrued_days: float
    taken_days: float
    balance_days: float
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
