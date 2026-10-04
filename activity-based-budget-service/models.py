"""Pydantic models for the Activity-Based Budget Service."""

import uuid
from datetime import datetime, timezone
from typing import List

from pydantic import BaseModel, Field


class Activity(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    cost_pool: str
    driver: str  # e.g. machine_hours, labor_hours, transactions, setups
    driver_rate: float = 0.0


class BudgetLineItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    activity_id: str
    period: str  # YYYY-MM
    expected_driver_volume: float
    budgeted_cost: float = 0.0
    notes: str = ""


class ActivityBudget(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    fiscal_year: str
    period: str  # YYYY-MM or full year
    line_items: List[BudgetLineItem] = []
    total_budget: float = 0.0
    status: str = "draft"  # draft, approved, actual
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
