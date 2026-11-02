"""Pydantic models for the Limiting Factor Service."""

import uuid
from datetime import datetime
from enum import Enum
from typing import Dict, List

from pydantic import BaseModel, Field


class LimitingFactorType(str, Enum):
    MACHINE_HOURS = "machine_hours"
    LABOR_HOURS = "labor_hours"
    MATERIAL_KG = "material_kg"
    MATERIAL_UNITS = "material_units"
    SPACE = "space"
    BUDGET = "budget"


class ProductWithFactor(BaseModel):
    product_id: str
    product_name: str
    demand: float
    contribution_per_unit: float
    factor_per_unit: float  # Resource usage per unit
    contribution_per_factor_unit: float = 0
    ranking: int = 0
    units_to_produce: float = 0
    total_contribution: float = 0


class LimitingFactorAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    analysis_date: datetime
    limiting_factor: str
    factor_type: str
    total_available: float
    factor_used: float = 0
    factor_remaining: float = 0
    products: List[ProductWithFactor] = []
    total_contribution: float = 0
    optimal_production_plan: Dict[str, float] = {}  # product_id -> units
    created_at: datetime = Field(default_factory=datetime.utcnow)
