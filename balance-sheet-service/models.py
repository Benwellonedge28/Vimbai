"""Pydantic models for the Balance Sheet Service."""

import uuid
from datetime import datetime, timezone
from typing import List, Optional

from pydantic import BaseModel, Field


class AssetItem(BaseModel):
    name: str
    amount: float
    category: str = "current"  # current, non_current
    is_liquid: bool = False


class LiabilityItem(BaseModel):
    name: str
    amount: float
    category: str = "current"  # current, non_current
    due_date: Optional[datetime] = None


class EquityItem(BaseModel):
    name: str
    amount: float


class BalanceSheetCreate(BaseModel):
    """Payload for generating a balance sheet (totals computed server side)."""

    company_id: str
    as_of_date: Optional[datetime] = None
    assets: List[AssetItem] = []
    liabilities: List[LiabilityItem] = []
    equity: List[EquityItem] = []


class BalanceSheet(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    as_of_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    assets: List[AssetItem] = []
    liabilities: List[LiabilityItem] = []
    equity: List[EquityItem] = []
    total_assets: float = 0
    total_liabilities: float = 0
    total_equity: float = 0
    is_balanced: bool = False
    created_at: Optional[datetime] = None
    updated_at: Optional[datetime] = None


def compute_totals(sheet: BalanceSheet) -> None:
    """Fill total_* and is_balanced in place (server-side computation)."""
    sheet.total_assets = sum(a.amount for a in sheet.assets)
    sheet.total_liabilities = sum(l.amount for l in sheet.liabilities)
    sheet.total_equity = sum(e.amount for e in sheet.equity)
    sheet.is_balanced = abs(sheet.total_assets - (sheet.total_liabilities + sheet.total_equity)) < 0.01
