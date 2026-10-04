"""Pydantic models for the Absorption Costing Service."""

import uuid
from datetime import datetime
from typing import List, Optional

from pydantic import BaseModel, Field


class CostComponent(BaseModel):
    component_name: str
    amount: float
    cost_type: str  # direct_material, direct_labor, direct_expense, manufacturing_overhead
    absorbed: bool = True


class ProductCost(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    product_name: str
    period: str
    direct_materials: float = 0
    direct_labor: float = 0
    direct_expenses: float = 0
    prime_cost: float = 0
    manufacturing_overhead: float = 0
    total_production_cost: float = 0
    units_produced: int = 0
    cost_per_unit: float = 0
    opening_stock: int = 0
    closing_stock: int = 0
    cost_components: List[CostComponent] = []
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class OverheadAbsorption(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    period: str
    overhead_cost: float
    absorption_base: str  # machine_hours, labor_hours, units, etc.
    absorption_base_units: float
    overhead_absorption_rate: float = 0
    absorbed_overhead: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)
