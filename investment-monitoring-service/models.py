"""Pydantic models for the Investment Monitoring Service.

:Portfolio via :OWNS_PORTFOLIO, :InvestmentHolding via :OWNS_HOLDING and
:PerformanceMetric via :OWNS_PERFORMANCE_METRIC, each carrying its portfolio id.
All Book-gated.
"""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class InvestmentHolding(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    portfolio_id: str
    instrument_name: str
    instrument_type: str  # equity, bond, etf, commodity, cash
    quantity: float
    purchase_price: float
    current_price: float
    currency: str = "USD"
    sector: str = ""
    updated_at: datetime = Field(default_factory=_utcnow)


class Portfolio(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    name: str
    description: str = ""
    target_return: float = 0.0
    risk_tolerance: str = "moderate"  # conservative, moderate, aggressive
    created_at: datetime = Field(default_factory=_utcnow)


class PerformanceMetric(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    portfolio_id: str
    metric_date: datetime
    total_value: float
    total_cost: float
    unrealized_gain: float
    unrealized_gain_pct: float
    daily_return: float = 0.0
    cumulative_return: float = 0.0
