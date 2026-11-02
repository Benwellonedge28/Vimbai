"""Pydantic models for the Prime Cost Service."""

import uuid
from datetime import datetime
from typing import Dict, Optional

from pydantic import BaseModel, Field


class DirectCostItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    item_name: str
    item_type: str  # direct_material, direct_labor, direct_expense
    amount: float
    units: float = 1
    cost_per_unit: float = 0
    product_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class PrimeCostCalculation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    period: str
    direct_materials: float = 0
    direct_labor: float = 0
    direct_expenses: float = 0
    prime_cost: float = 0
    cost_breakdown: Dict[str, float] = {}
    created_at: datetime = Field(default_factory=datetime.utcnow)
