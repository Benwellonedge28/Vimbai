"""Pydantic models for the Intangible Assets Service.

Assets and the amortisation charged against them are nodes: :IntangibleAsset via
:OWNS_INTANGIBLE_ASSET and :AmortizationEntry via :OWNS_AMORTIZATION_ENTRY, each
stamped with the caller and the Book.
"""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class IntangibleAsset(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    name: str
    asset_type: str  # goodwill, patent, trademark, copyright, software, license
    cost: float
    useful_life_years: int
    residual_value: float = 0.0
    acquisition_date: datetime
    amortization_method: str = "straight_line"  # straight_line, reducing_balance
    accumulated_amortization: float = 0.0
    net_book_value: float = 0.0
    status: str = "active"  # active, fully_amortized, impaired, disposed
    created_at: datetime = Field(default_factory=_utcnow)


class AmortizationEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    asset_id: str
    period: str
    amortization_amount: float
    accumulated_amortization: float
    net_book_value: float
    created_at: datetime = Field(default_factory=_utcnow)
