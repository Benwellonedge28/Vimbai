"""Pydantic models for the Equity Changes Service."""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


class EquityTransactionType(str, Enum):
    ISSUANCE = "issuance"
    BUYBACK = "buyback"
    DIVIDEND = "dividend"
    SPLIT = "split"
    TRANSFER = "transfer"
    RETAINED = "retained_earnings"


class EquityTransactionCreate(BaseModel):
    company_id: str
    transaction_type: EquityTransactionType
    shareholder: str = ""
    shares: int = 0
    price_per_share: float = 0
    amount: float = 0
    description: str = ""
    date: Optional[datetime] = None


class EquityTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    transaction_type: EquityTransactionType
    shareholder: str = ""
    shares: int = 0
    price_per_share: float = 0
    amount: float = 0
    description: str = ""
    date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    created_at: Optional[datetime] = None


class EquityStatementCreate(BaseModel):
    """Payload for generating a statement (totals computed server side)."""

    company_id: str
    period: str
    beginning_equity: float
    transactions: List[EquityTransactionCreate] = []


class EquityStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    period: str
    beginning_equity: float
    share_issuances: float = 0
    share_buybacks: float = 0
    dividends_paid: float = 0
    retained_earnings_change: float = 0
    other_changes: float = 0
    ending_equity: float = 0
    transactions: List[EquityTransaction] = []
    created_at: Optional[datetime] = None
