"""Pydantic models for the Selling Price Reduction Service."""

import uuid
from datetime import datetime

from pydantic import BaseModel, Field


class PriceReductionAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    product_name: str
    current_price: float
    proposed_price: float
    price_reduction: float = 0
    price_reduction_percentage: float = 0
    current_contribution_per_unit: float = 0
    new_contribution_per_unit: float = 0
    contribution_change_per_unit: float = 0
    current_expected_sales: float = 0
    new_expected_sales: float = 0
    current_total_contribution: float = 0
    new_total_contribution: float = 0
    contribution_difference: float = 0
    minimum_price_for_target_contribution: float = 0
    recommendation: str = ""
    break_even_volume_increase: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)
