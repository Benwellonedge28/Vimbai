"""Pydantic models for the Fixed Assets Schedule Service.

The register and the schedule it produces are nodes: :FixedAsset via
:OWNS_FIXED_ASSET and :AssetSchedule via :OWNS_ASSET_SCHEDULE, both stamped with
the caller and the Book.
"""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Dict, List, Optional

from pydantic import BaseModel, Field


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class AssetCategory(str, Enum):
    LAND = "land"
    BUILDINGS = "buildings"
    PLANT_MACHINERY = "plant_machinery"
    FURNITURE_FIXTURES = "furniture_fixtures"
    MOTOR_VEHICLES = "motor_vehicles"
    OFFICE_EQUIPMENT = "office_equipment"
    COMPUTER_EQUIPMENT = "computer_equipment"
    LEASEHOLD_IMPROVEMENTS = "leasehold_improvements"
    INTANGIBLE_ASSETS = "intangible_assets"


class AssetStatus(str, Enum):
    IN_USE = "in_use"
    UNDER_MAINTENANCE = "under_maintenance"
    IDLE = "idle"
    DISPOSED = "disposed"
    SCRAPPED = "scrapped"


class FixedAsset(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    asset_code: str
    asset_name: str
    description: Optional[str] = None
    category: AssetCategory
    location: Optional[str] = None
    custodian: Optional[str] = None
    purchase_date: datetime
    purchase_cost: float
    residual_value: float = 0
    useful_life_years: int
    depreciation_method: str = "straight_line"
    accumulated_depreciation: float = 0
    net_book_value: float = 0
    status: AssetStatus = AssetStatus.IN_USE
    supplier: Optional[str] = None
    warranty_expiry: Optional[datetime] = None
    insurance_policy: Optional[str] = None
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=_utcnow)
    updated_at: datetime = Field(default_factory=_utcnow)


class AssetSchedule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    as_of_date: datetime
    assets: List[FixedAsset] = []
    total_cost: float = 0
    total_accumulated_depreciation: float = 0
    total_net_book_value: float = 0
    category_summary: Dict[str, Dict[str, float]] = {}
    created_at: datetime = Field(default_factory=_utcnow)
