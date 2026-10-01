"""Pydantic models for the Cash Optimization Service."""

import uuid
from enum import Enum
from typing import Optional

from pydantic import BaseModel, Field


class AccountType(str, Enum):
    OPERATING = "operating"
    RESERVE = "reserve"
    INVESTMENT = "investment"
    TAX = "tax"
    PAYROLL = "payroll"


class CashAccount(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    account_name: str
    account_type: AccountType
    balance: float
    min_required: float = 0
    interest_rate: float = 0
    currency: str = "USD"


class CashAccountCreate(BaseModel):
    company_id: str
    account_name: str
    account_type: AccountType
    balance: float
    min_required: float = 0
    interest_rate: float = 0
    currency: str = "USD"


class OptimizationSuggestion(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    from_account: str
    to_account: str
    amount: float
    reason: str
    expected_benefit: float = 0  # annual benefit
    priority: str = "medium"  # low, medium, high
