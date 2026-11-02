"""Pydantic models for the Lifecycle Costing Service."""

import uuid
from datetime import datetime, timezone
from typing import List

from pydantic import BaseModel, Field


class LifecyclePhase(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    phase: str  # acquisition, operation, maintenance, disposal
    description: str = ""
    duration_years: int = 0
    annual_cost: float = 0.0
    one_time_cost: float = 0.0


class LifecycleCostModel(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_name: str
    description: str = ""
    phases: List[LifecyclePhase] = []
    total_lifecycle_cost: float = 0.0
    annual_equivalent_cost: float = 0.0
    discount_rate: float = 0.1
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
