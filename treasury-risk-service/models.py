"""Pydantic models for Treasury Risk Service (API contract unchanged)."""

import uuid
from datetime import datetime, timezone

from pydantic import BaseModel, Field


class RiskExposure(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    exposure_type: str  # fx, interest_rate, credit, liquidity, commodity
    currency: str = "USD"
    notional_amount: float
    description: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class VaRResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    portfolio_value: float
    confidence_level: float  # e.g. 0.95, 0.99
    holding_period_days: int
    var_amount: float
    var_pct: float
    method: str = "parametric"  # parametric, historical, monte_carlo
    calculated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class StressTestScenario(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    shock_type: str  # interest_rate_up, interest_rate_down, fx_devaluation, market_crash
    shock_magnitude: float  # basis points or percentage
    portfolio_impact: float = 0.0
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class StressTestResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    scenario_id: str
    portfolio_value_before: float
    portfolio_value_after: float
    impact: float
    impact_pct: float
    tested_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
