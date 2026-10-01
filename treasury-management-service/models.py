"""Pydantic models for the Treasury Management Service."""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


class CashFlowType(str, Enum):
    INFLOW = "inflow"
    OUTFLOW = "outflow"
    INVESTMENT = "investment"
    FINANCING = "financing"


class LiquidityLevel(str, Enum):
    EXCESS = "excess"
    ADEQUATE = "adequate"
    TIGHT = "tight"
    CRITICAL = "critical"


class CashFlowEntryCreate(BaseModel):
    company_id: str
    account_id: str = ""
    flow_type: CashFlowType
    amount: float
    currency: str = "USD"
    date: Optional[datetime] = None
    description: str = ""
    category: str = ""


class CashFlowEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    account_id: str = ""
    flow_type: CashFlowType
    amount: float
    currency: str = "USD"
    date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    description: str = ""
    category: str = ""


class CashPositionUpdate(BaseModel):
    total_cash: float
    currency: str = "USD"
    available_cash: float
    restricted_cash: float = 0
    short_term_investments: float = 0
    as_of: Optional[datetime] = None


class CashPosition(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    total_cash: float
    currency: str = "USD"
    available_cash: float
    restricted_cash: float = 0
    short_term_investments: float = 0
    liquidity_level: LiquidityLevel = LiquidityLevel.ADEQUATE
    as_of: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CashFlowForecast(BaseModel):
    company_id: str
    period_start: datetime
    period_end: datetime
    projected_inflows: float
    projected_outflows: float
    net_cash_flow: float
    ending_position: float
    confidence: float = 0.75
    assumptions: List[str] = []


class InvestmentOption(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    instrument_type: str  # money_market, treasury_bill, bond, fixed_deposit
    expected_return: float
    duration_days: int
    min_amount: float
    risk_level: str = "low"
    liquidity: str = "high"
