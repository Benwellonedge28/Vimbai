"""Pydantic models for the Marginal Costing Service."""

import uuid
from datetime import datetime
from typing import Dict

from pydantic import BaseModel, Field


class MarginalCost(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_name: str
    cost_code: str
    variable_cost: float  # Marginal/Variable cost
    fixed_cost: float = 0  # Period cost
    total_cost: float = 0
    unit_variable_cost: float = 0
    units: float = 1
    cost_driver: str
    created_at: datetime = Field(default_factory=datetime.utcnow)


class MarginalIncomeStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period: str
    sales_revenue: float
    total_variable_costs: float
    contribution: float = 0
    fixed_costs: float
    profit: float = 0
    variable_cost_breakdown: Dict[str, float] = {}
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ContributionAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    selling_price: float
    variable_cost_per_unit: float
    contribution_per_unit: float = 0
    contribution_margin_ratio: float = 0
    total_contribution: float = 0
    fixed_costs_allocated: float = 0
    profit_from_product: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)
