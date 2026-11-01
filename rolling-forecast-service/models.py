"""
Rolling Forecast Service models.

The forecast itself is a :RollingForecast node; its periods ride on the
node as a JSON prop and are appended/recalculated in Python, which keeps
the arithmetic identical to the in-memory version.
"""

from datetime import datetime, timezone
from typing import List, Optional
from uuid import uuid4

from pydantic import BaseModel, Field

VALID_FREQUENCIES = ["weekly", "monthly", "quarterly"]


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


def _new_id() -> str:
    return str(uuid4())


class ForecastPeriod(BaseModel):
    id: str = Field(default_factory=_new_id)
    period: str  # YYYY-MM
    forecast_value: float
    actual_value: Optional[float] = None
    variance: Optional[float] = None
    variance_pct: Optional[float] = None
    is_actual: bool = False


class RollingForecast(BaseModel):
    id: str = Field(default_factory=_new_id)
    name: str
    metric: str  # revenue, expenses, cash_flow, headcount
    frequency: str = "monthly"  # weekly, monthly, quarterly
    horizon_periods: int = 12
    periods: List[ForecastPeriod] = []
    current_period: str = ""
    last_updated: datetime = Field(default_factory=_utcnow)
    created_at: datetime = Field(default_factory=_utcnow)
