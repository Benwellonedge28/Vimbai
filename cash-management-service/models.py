"""Pydantic models for Cash Management Service (API contract unchanged)."""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


class CashAccount(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    account_name: str
    bank: str
    account_number: str
    currency: str = "USD"
    balance: float = 0.0
    min_balance: float = 0.0
    type: str = "operating"  # operating, reserve, investment
    status: str = "active"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CashTransfer(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    from_account_id: str
    to_account_id: str
    amount: float
    currency: str = "USD"
    transfer_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    status: str = "pending"  # pending, completed, failed
    reference: str = ""
    notes: str = ""


class LiquidityPosition(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    position_date: datetime
    total_cash: float
    operating_cash: float
    reserve_cash: float
    invested_cash: float
    short_term_obligations: float
    liquidity_ratio: float = 0.0
