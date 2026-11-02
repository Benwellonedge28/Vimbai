"""Pydantic models for the Overhead Apportionment Service."""

import uuid
from datetime import datetime
from typing import Any, Dict, List

from pydantic import BaseModel, Field


class OverheadItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    overhead_name: str
    overhead_code: str
    total_amount: float
    basis: str  # floor_area, volume, nbv, personnel, requisitions, etc.
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ApportionedOverhead(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    overhead_id: str
    period: str
    cost_centre_apportionments: List[Dict[str, Any]] = []
    total_overhead: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)
