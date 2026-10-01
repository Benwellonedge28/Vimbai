"""Pydantic models for the Scenario Analysis Service."""

import uuid
from typing import Optional

from pydantic import BaseModel, Field


class ScenarioAssumption(BaseModel):
    revenue_growth: float = 0.1
    cost_growth: float = 0.05
    interest_rate: float = 0.05
    tax_rate: float = 0.25
    capex: float = 0
    description: str = ""


class ScenarioRequest(BaseModel):
    company_id: str
    base_revenue: float
    base_cost: float
    base_interest: float = 0
    base_depreciation: float = 0
    best_case: ScenarioAssumption
    base_case: ScenarioAssumption
    worst_case: ScenarioAssumption


class ScenarioResult(BaseModel):
    name: str
    description: str
    projected_revenue: float
    projected_cost: float
    ebit: float
    pretax_income: float
    net_income: float
    net_margin: float


class AnalysisResponse(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    best_case: ScenarioResult
    base_case: ScenarioResult
    worst_case: ScenarioResult
    sensitivity_revenue: float
    sensitivity_cost: float
    recommendation: str


class Scenario(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    name: str
    scenario_type: str = "custom"
    projected_revenue: float = 0
    projected_expenses: float = 0
    net_projection: float = 0


class ScenarioCreate(BaseModel):
    company_id: str
    name: str
    scenario_type: str = "custom"
    projected_revenue: float = 0
    projected_expenses: float = 0
