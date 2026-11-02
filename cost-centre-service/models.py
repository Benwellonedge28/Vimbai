"""Pydantic models for the Cost Centre Service."""

import uuid
from datetime import datetime
from enum import Enum
from typing import Optional

from pydantic import BaseModel, Field


class CostCentreType(str, Enum):
    PRODUCTION = "production"
    SERVICE = "service"
    ADMINISTRATIVE = "administrative"


class CostCentre(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    centre_code: str
    centre_name: str
    centre_type: str  # production, service, administrative
    department_id: Optional[str] = None
    floor_area: float = 0  # Square meters
    number_of_personnel: int = 0
    number_of_requisitions: int = 0
    machine_value: float = 0  # Value of machinery
    is_service_centre: bool = False
    parent_centre_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CostCentreCost(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_centre_id: str
    cost_code: str
    cost_description: str
    amount: float
    cost_type: str  # direct, indirect, overhead
    period: str
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CostCentreSummary(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_centre_id: str
    period: str
    total_direct_costs: float = 0
    total_indirect_costs: float = 0
    total_overhead: float = 0
    allocated_service_costs: float = 0
    total_cost: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)
