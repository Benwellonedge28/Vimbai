"""Pydantic models for the Trading Account Service.

A generated trading account is a record, not a transient calculation, so it is a
node: :TradingAccount via :OWNS_TRADING_ACCOUNT, stamped with the caller and the
Book.
"""

import uuid
from datetime import datetime, timezone
from typing import Dict, Optional

from pydantic import BaseModel, Field


class TradingAccountRequest(BaseModel):
    company_id: str = ""
    period: str = ""
    opening_stock: float = 0.0
    purchases: float = 0.0
    carriage_inward: float = 0.0
    closing_stock: float = 0.0
    sales: float = 0.0
    sales_returns: float = 0.0
    direct_wages: float = 0.0
    factory_overhead: float = 0.0


class TradingAccountResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str = ""
    period: str = ""
    net_sales: float = 0.0
    cost_of_goods_sold: float = 0.0
    gross_profit: float = 0.0
    gross_profit_pct: float = 0.0
    details: Dict[str, float] = {}
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
