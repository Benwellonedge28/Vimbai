"""Pydantic models for the Initial Investment Service."""

import uuid
from datetime import datetime
from typing import List

from pydantic import BaseModel, Field


class InvestmentDetail(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    item: str
    amount: float
    category: str  # asset, working_capital, installation, other


class InitialInvestment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    project_name: str
    items: List[InvestmentDetail]
    total_investment: float = 0
    scrap_value: float = 0
    net_investment: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)
