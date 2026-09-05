"""Pydantic models for the Fund Accounting Service."""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


class FundCreate(BaseModel):
    company_id: str
    fund_name: str
    fund_type: str = "general"  # general, restricted, endowment, project
    balance: float = 0
    income: float = 0
    expenses: float = 0
    restrictions: str = ""
    manager: str = ""


class Fund(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    fund_name: str
    fund_type: str = "general"
    balance: float = 0
    income: float = 0
    expenses: float = 0
    net_assets: float = 0
    restrictions: str = ""
    manager: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class FundTransactionCreate(BaseModel):
    fund_id: str
    description: str
    amount: float
    is_income: bool
    category: str = ""


class FundTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    fund_id: str
    description: str
    amount: float
    is_income: bool
    category: str = ""
    date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
