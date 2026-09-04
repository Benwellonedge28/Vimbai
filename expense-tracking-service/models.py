"""Pydantic models for Expense Tracking Service"""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Optional

from pydantic import BaseModel, Field


class ExpenseCategory(str, Enum):
    TRAVEL = "travel"
    MEALS = "meals"
    OFFICE = "office"
    UTILITIES = "utilities"
    SOFTWARE = "software"
    EQUIPMENT = "equipment"
    PROFESSIONAL = "professional"
    MARKETING = "marketing"
    OTHER = "other"


class ExpenseStatus(str, Enum):
    PENDING = "pending"
    APPROVED = "approved"
    REJECTED = "rejected"
    REIMBURSED = "reimbursed"


class ExpenseBase(BaseModel):
    company_id: str
    employee_id: str
    category: ExpenseCategory
    amount: float = Field(gt=0)
    currency: str = "USD"
    date: Optional[datetime] = None
    description: str = ""
    vendor: str = ""
    receipt_url: str = ""
    project_code: str = ""


class ExpenseCreate(ExpenseBase):
    """Payload for creating an expense."""

    pass


class Expense(ExpenseBase):
    """Expense record as stored and returned by the API."""

    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    status: ExpenseStatus = ExpenseStatus.PENDING
    approved_by: str = ""
    rejection_reason: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ExpenseSummary(BaseModel):
    company_id: str
    total_expenses: int
    total_amount: float
    by_category: dict
    by_status: dict
