"""Pydantic models for the Process Costing Service."""

import uuid
from datetime import datetime, timezone
from typing import List, Optional

from pydantic import BaseModel, Field


class CostComponent(BaseModel):
    name: str
    amount: float
    cost_type: str = "direct"  # direct_materials, direct_labor, overhead, etc.


class CostCalculation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    product_or_process: str
    period: str = ""
    components: List[CostComponent] = []
    total_cost: float = 0
    unit_cost: float = 0
    quantity: int = 1
    notes: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CostCalculationCreate(BaseModel):
    company_id: str
    product_or_process: str
    period: str = ""
    components: List[CostComponent] = []
    quantity: int = 1
    notes: str = ""
