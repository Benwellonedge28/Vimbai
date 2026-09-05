"""Pydantic models for the Job Costing Service."""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field

COST_TYPES = ("materials", "labor", "overhead", "subcontractor")


class JobCreate(BaseModel):
    company_id: str
    job_name: str
    customer: str = ""
    contract_value: float = 0
    status: str = "active"
    start_date: Optional[datetime] = None
    end_date: Optional[datetime] = None


class Job(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    job_name: str
    customer: str = ""
    contract_value: float = 0
    status: str = "active"  # active, completed, billed, closed
    start_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    end_date: Optional[datetime] = None
    materials_cost: float = 0
    labor_cost: float = 0
    overhead_cost: float = 0
    subcontractor_cost: float = 0
    total_cost: float = 0
    gross_profit: float = 0
    gross_margin: float = 0
    created_at: Optional[datetime] = None


class JobCostEntryCreate(BaseModel):
    cost_type: str  # materials, labor, overhead, subcontractor
    amount: float
    date: Optional[datetime] = None
    description: str = ""


class JobCostEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    job_id: str = ""
    cost_type: str
    amount: float
    date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    description: str = ""
    created_at: Optional[datetime] = None
