"""Pydantic models for the Fixed Cost Service.

A cost item is a node; an analysis is a node too, because the service keeps the
reports it has produced and lets a caller list them again later.
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


class FixedCost(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    cost_name: str
    cost_code: str
    amount: float
    cost_behavior: str = "fixed"
    relevant_range_min: float = 0
    relevant_range_max: float = 0
    period_type: str = "monthly"
    committed_cost: bool = False
    controllable: bool = True
    department_id: Optional[str] = None
    product_id: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class FixedCostAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    analysis_period_start: datetime
    analysis_period_end: datetime
    total_fixed_costs: float = 0
    committed_costs: float = 0
    discretionary_costs: float = 0
    controllable_costs: float = 0
    non_controllable_costs: float = 0
    fixed_cost_details: List[Dict[str, Any]] = []
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
