"""Pydantic models for Revaluation Reserve Service (API contract unchanged)."""

import uuid
from datetime import datetime
from typing import List, Optional

from pydantic import BaseModel, Field


class RevaluationEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    asset_id: str
    asset_name: str
    asset_class: str  # property, plant, equipment, investment_property
    revaluation_date: datetime
    previous_value: float
    new_value: float
    revaluation_gain: float = 0
    revaluation_loss: float = 0
    depreciation_adjustment: float = 0  # Adjustment to accumulated depreciation
    net_effect: float = 0
    journal_entry_id: Optional[str] = None
    status: str = "completed"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class RevaluationUtilization(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    amount: float
    utilization_type: str  # asset_disposal, impairment, transfer_retained_earnings
    related_asset_id: Optional[str] = None
    description: str
    journal_entry_id: Optional[str] = None
    utilization_date: datetime
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CumulativeRevaluation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    asset_id: str
    total_revaluation_gain: float = 0
    total_revaluation_loss: float = 0
    total_utilized: float = 0
    net_revaluation_reserve: float = 0
    last_revaluation_date: Optional[datetime] = None
