"""Pydantic models for Treasury Reporting Service (API contract unchanged)."""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict

from pydantic import BaseModel, Field


class TreasuryReport(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    report_type: str  # cash_position, fx_exposure, debt_portfolio, liquidity, investment_summary
    period: str  # YYYY-MM or YYYY-Qn
    data: Dict[str, Any] = {}
    summary: Dict[str, Any] = {}
    generated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    generated_by: str = ""


class CashPositionEntry(BaseModel):
    account_id: str
    account_name: str
    currency: str
    balance: float
    balance_usd: float = 0.0


class FXExposureEntry(BaseModel):
    currency_pair: str
    exposure_amount: float
    exposure_usd: float = 0.0
    hedge_ratio: float = 0.0
    unhedged_amount: float = 0.0
