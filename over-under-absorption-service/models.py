"""Pydantic models for the Over/Under Absorption Service."""

import uuid
from datetime import datetime

from pydantic import BaseModel, Field


class OverUnderAbsorption(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_centre_name: str
    cost_centre_id: str
    period: str
    budgeted_overhead: float
    actual_overhead: float
    budgeted_base_units: float
    actual_base_units: float
    absorption_rate: float
    overhead_absorbed: float
    over_absorption: float = 0
    under_absorption: float = 0
    total_variance: float = 0
    cause: str = ""  # overhead_spending, volume, efficiency
    created_at: datetime = Field(default_factory=datetime.utcnow)


class VarianceAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_centre_name: str
    period: str
    overhead_absorbed: float
    actual_overhead: float
    spending_variance: float = 0  # budget vs actual overhead
    volume_variance: float = 0  # actual vs budgeted activity
    capacity_variance: float = 0  # idle capacity
    efficiency_variance: float = 0  # actual vs standard rate
    total_variance: float = 0
    over_or_under: str = ""
    created_at: datetime = Field(default_factory=datetime.utcnow)
