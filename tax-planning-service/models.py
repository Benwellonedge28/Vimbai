"""Pydantic models for the Tax Planning Service."""

import uuid
from typing import Dict, List

from pydantic import BaseModel, Field


class TaxStrategy(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str
    strategy_type: str  # deduction, credit, timing, structure, treaty
    estimated_savings: float
    implementation_cost: float = 0
    risk_level: str = "low"
    timeframe: str = "short-term"


class PlanningRequest(BaseModel):
    company_id: str
    fiscal_year: int
    current_taxable_income: float
    current_tax: float
    strategies: List[TaxStrategy] = []


class StrategyDetail(BaseModel):
    id: str
    name: str
    type: str
    estimated_savings: float
    implementation_cost: float
    net_benefit: float
    risk_level: str
    timeframe: str
    roi: float = None


class PlanningResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    fiscal_year: int
    current_tax: float
    projected_tax: float
    total_savings: float
    net_benefit: float
    strategies: List[Dict] = []
    recommended_strategies: List[str] = []
