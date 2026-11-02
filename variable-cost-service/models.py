"""Pydantic models for the Variable Cost Service.

A cost item carries the total it produced at the quantity it was added with; the
service recomputes that total whenever the unit cost or quantity changes. Analyses
are kept so the same report can be read again later.
"""

import uuid
from datetime import datetime, timezone
from typing import Dict, Optional

from pydantic import BaseModel, Field


class VariableCost(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    cost_name: str
    cost_code: str
    unit_cost: float
    total_cost: float = 0
    quantity: float = 0
    cost_driver: str
    cost_driver_rate: float
    department_id: Optional[str] = None
    product_id: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class VariableCostAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    analysis_period: str
    total_variable_costs: float = 0
    total_units: float = 0
    average_cost_per_unit: float = 0
    cost_driver_analysis: Dict[str, float] = {}
    product_costs: Dict[str, float] = {}
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
