"""Pydantic models for the Overhead Absorption Rate Service.

:OverheadAbsorptionRate via :OWNS_ABSORPTION_RATE and :OARAnalysis via
:OWNS_OAR_ANALYSIS, both Book-gated.
"""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class OverheadAbsorptionRate(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    cost_centre_name: str
    cost_centre_id: str
    overhead_type: str  # production, service, administrative
    absorption_base: str  # machine_hours, direct_labour_hours, direct_labour_cost, material_cost, etc.
    budgeted_overhead: float
    budgeted_base_units: float
    absorption_rate: float = 0
    rate_per_unit: float = 0
    created_at: datetime = Field(default_factory=_utcnow)


class OARAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    entity_name: str
    cost_centre_name: str
    rate_type: str  # blanket, departmental
    absorption_base: str
    calculated_rate: float
    base_units_used: float
    overhead_absorbed: float
    created_at: datetime = Field(default_factory=_utcnow)
