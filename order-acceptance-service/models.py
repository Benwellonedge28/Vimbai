"""Pydantic model for the Order Acceptance Service."""

import uuid
from datetime import datetime
from typing import List

from pydantic import BaseModel, Field


class OrderAcceptanceDecision(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    order_id: str
    product_id: str
    product_name: str
    customer_name: str
    normal_selling_price: float
    offered_price: float
    price_below_normal: float = 0
    price_below_normal_percentage: float = 0
    variable_cost_per_unit: float
    full_cost_per_unit: float = 0
    contribution_per_unit_at_normal: float = 0
    contribution_per_unit_at_offered: float = 0
    contribution_from_order: float = 0
    total_fixed_cost_incremental: float = 0
    units_requested: float
    total_relevant_cost: float = 0
    minimum_acceptable_price: float = 0
    opportunity_cost: float = 0  # Contribution lost from displaced regular sales
    net_contribution: float = 0
    recommendation: str = ""
    conditions: List[str] = []
    created_at: datetime = Field(default_factory=datetime.utcnow)
