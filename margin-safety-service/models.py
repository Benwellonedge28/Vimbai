"""Pydantic model for the Margin of Safety Service."""

import uuid
from datetime import datetime

from pydantic import BaseModel, Field


class MarginOfSafety(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    entity_id: str
    entity_name: str
    period: str
    current_output: float
    current_revenue: float
    break_even_output: float
    break_even_revenue: float
    margin_of_safety_units: float = 0
    margin_of_safety_revenue: float = 0
    margin_of_safety_percentage: float = 0
    margin_of_safety_ratio: float = 0
    risk_level: str = ""  # low, medium, high
    created_at: datetime = Field(default_factory=datetime.utcnow)
