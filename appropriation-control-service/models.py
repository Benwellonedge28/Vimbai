"""Pydantic models for the Appropriation Control Service."""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


class Appropriation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    department: str
    fiscal_year: str
    approved_amount: float
    spent_amount: float = 0
    committed_amount: float = 0
    available_amount: float = 0
    status: str = "active"  # active, exhausted, closed


class AppropriationCreate(BaseModel):
    company_id: str
    department: str
    fiscal_year: str
    approved_amount: float
    spent_amount: float = 0
    committed_amount: float = 0


class AppropriationTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    appropriation_id: str
    type: str = "commit"  # commit, spend, uncommit, refund
    amount: float
    description: str = ""
    date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class AppropriationTransactionCreate(BaseModel):
    appropriation_id: str
    type: str = "commit"  # commit, spend, uncommit, refund
    amount: float
    description: str = ""
