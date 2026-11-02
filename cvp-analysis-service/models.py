"""Pydantic models for the CVP Analysis Service."""

import uuid
from datetime import datetime
from typing import List

from pydantic import BaseModel, Field


class CVPAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    entity_id: str
    entity_name: str
    analysis_date: datetime

    # Inputs
    selling_price_per_unit: float
    variable_cost_per_unit: float
    fixed_costs: float
    expected_sales_units: float = 0.0
    target_profit: float = 0.0

    # Contribution
    contribution_per_unit: float = 0.0
    contribution_margin_ratio: float = 0.0

    # Break-even
    break_even_units: float = 0.0
    break_even_revenue: float = 0.0

    # Target profit
    units_for_target_profit: float = 0.0
    revenue_for_target_profit: float = 0.0

    # Margin of safety
    expected_sales_revenue: float = 0.0
    margin_of_safety_units: float = 0.0
    margin_of_safety_revenue: float = 0.0
    margin_of_safety_percentage: float = 0.0

    # Additional metrics
    profit_at_expected_sales: float = 0.0
    degree_of_operating_leverage: float = 0.0

    created_at: datetime = Field(default_factory=datetime.utcnow)


class ContributionRequest(BaseModel):
    entity_id: str
    entity_name: str
    selling_price_per_unit: float
    variable_cost_per_unit: float
    units_sold: float = 0.0


class MultiProductItem(BaseModel):
    product_name: str
    selling_price: float
    variable_cost: float
    expected_proportion: float  # percentage of total sales mix


class MultiProductRequest(BaseModel):
    entity_id: str
    entity_name: str
    fixed_costs: float
    products: List[MultiProductItem]
    target_profit: float = 0.0
