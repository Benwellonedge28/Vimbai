"""Pydantic models for the Continue/Shutdown Decision Service."""

import uuid
from datetime import datetime

from pydantic import BaseModel, Field


class CostAnalysis(BaseModel):
    # If continuing
    revenue: float = 0
    variable_costs: float = 0
    contribution: float = 0
    avoidable_fixed_costs: float = 0  # Fixed costs saved if shutdown
    unavoidable_fixed_costs: float = 0  # Costs that continue even if shutdown


class ContinueShutdownDecision(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    entity_id: str  # Department or product ID
    entity_name: str
    entity_type: str  # department, product, division
    period: str
    cost_analysis: CostAnalysis
    shutdown_costs: float = 0  # Additional costs from shutdown (redundancy, etc.)
    shutdown_savings: float = 0  # Savings from shutdown
    net_shutdown_benefit: float = 0
    contribution_if_continue: float = 0
    financial_outcome_continue: float = 0  # Profit if continue
    financial_outcome_shutdown: float = 0  # Profit if shutdown
    recommendation: str = ""
    created_at: datetime = Field(default_factory=datetime.utcnow)
