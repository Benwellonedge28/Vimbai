"""Pydantic models for the Sensitivity Analysis Service."""

import uuid
from datetime import datetime, timezone
from typing import List, Optional

from pydantic import BaseModel, Field


class Variable(BaseModel):
    name: str
    base_value: float
    change_pct: float = 0  # percentage change to test


class SensitivityResult(BaseModel):
    variable_name: str
    base_value: float
    changed_value: float
    change_pct: float
    impact_on_target: float
    elasticity: float = 0  # % change in target / % change in variable


class AnalysisRequest(BaseModel):
    company_id: str
    target_metric: str  # e.g., "net_profit", "cash_flow", "revenue"
    base_target_value: float
    variables: List[Variable]
    change_steps: List[float] = [-10, -5, 0, 5, 10]  # percentage changes to test


class AnalysisResponse(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    target_metric: str
    base_target_value: float
    results: List[SensitivityResult]
    most_sensitive_variable: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
