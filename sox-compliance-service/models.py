"""Pydantic models for SOX Compliance Service (API contract unchanged)."""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


class Control(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    control_id_ref: str  # e.g. SOX-ITGC-001
    description: str
    control_type: str  # preventive, detective, corrective
    control_nature: str  # manual, automated, IT-dependent
    frequency: str  # daily, weekly, monthly, quarterly, annual
    owner: str
    process: str
    risk_level: str = "medium"  # low, medium, high
    status: str = "active"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ControlTest(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    control_id: str
    test_period: str
    tester: str
    sample_size: int = 25
    exceptions_found: int = 0
    result: str = "pass"  # pass, fail, pass_with_exception
    test_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    notes: str = ""


class Deficiency(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    control_id: str
    severity: str  # control_deficiency, significant_deficiency, material_weakness
    description: str
    remediation_plan: str = ""
    remediation_owner: str = ""
    status: str = "open"  # open, in_progress, remediated
    identified_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    remediated_date: Optional[datetime] = None
