"""Pydantic models for the Disposal Group Service.

:DisposalGroup via :OWNS_DISPOSAL_GROUP, its assets as :DisposalAsset nodes hanging
off the group, and each impairment test as an :ImpairmentTest node owned by the
caller. All Book-gated.
"""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class DisposalAsset(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    group_id: Optional[str] = None
    asset_id: str
    asset_name: str
    carrying_amount: float
    fair_value: float = 0.0
    classification: str  # noncurrent_asset, subsidiary, business_segment


class DisposalGroup(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    name: str
    description: str = ""
    disposal_method: str = "sale"  # sale, abandonment, spin_off, exchange
    status: str = "held_for_sale"  # held_for_sale, sold, withdrawn
    classification_date: datetime = Field(default_factory=_utcnow)
    expected_disposal_date: Optional[datetime] = None
    total_carrying_amount: float = 0.0
    total_fair_value: float = 0.0
    impairment_loss: float = 0.0
    created_at: datetime = Field(default_factory=_utcnow)


class ImpairmentTest(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    group_id: str
    carrying_amount: float
    fair_value_less_costs: float
    impairment_amount: float
    tested_at: datetime = Field(default_factory=_utcnow)
    notes: str = ""
