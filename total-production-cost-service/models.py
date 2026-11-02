"""Pydantic models for the Total Production Cost Service."""

import uuid
from datetime import datetime

from pydantic import BaseModel, Field


class ProductionCostCalculation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    department_id: str
    period: str

    # Prime Cost Components
    direct_materials: float = 0
    direct_labor: float = 0
    direct_expenses: float = 0
    prime_cost: float = 0

    # Factory Overhead
    factory_rent: float = 0
    factory_depreciation: float = 0
    factory_insurance: float = 0
    factory_maintenance: float = 0
    other_overhead: float = 0
    total_overhead: float = 0

    # Work in Progress
    wip_opening: float = 0
    wip_closing: float = 0

    # Total Production Cost
    total_production_cost: float = 0
    units_produced: int = 0
    cost_per_unit: float = 0

    created_at: datetime = Field(default_factory=datetime.utcnow)
