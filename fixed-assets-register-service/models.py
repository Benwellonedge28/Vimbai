"""Pydantic models for Fixed Assets Register Service (API contract unchanged)."""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


class FixedAsset(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_code: str
    asset_name: str
    category: str  # land, buildings, vehicles, machinery, furniture, equipment, IT
    location: str = ""
    department: str = ""
    acquisition_date: datetime
    acquisition_cost: float
    useful_life_years: int
    salvage_value: float = 0.0
    depreciation_method: str = "straight_line"  # straight_line, reducing_balance, units_of_production
    accumulated_depreciation: float = 0.0
    net_book_value: float = 0.0
    status: str = "active"  # active, disposed, impaired, under_construction
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class DepreciationEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_id: str
    period: str  # YYYY-MM
    depreciation_amount: float
    accumulated_depreciation: float
    net_book_value: float
    method: str
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class AssetDisposal(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_id: str
    disposal_date: datetime
    disposal_value: float = 0.0
    disposal_method: str = "sale"  # sale, scrap, donation, write_off
    gain_loss: float = 0.0
    notes: str = ""
