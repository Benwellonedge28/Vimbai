"""Pydantic models for the Debt Management Service."""

import uuid
from datetime import date, datetime, timezone
from typing import Dict, List, Optional

from pydantic import BaseModel, Field


class LoanCreate(BaseModel):
    company_id: str
    loan_name: str
    lender: str
    principal: float
    interest_rate: float
    term_months: int
    disbursement_date: date
    payment_frequency: str = "monthly"
    status: str = "active"  # active, paid, defaulted, restructured


class Loan(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    loan_name: str
    lender: str
    principal: float
    interest_rate: float
    term_months: int
    disbursement_date: date
    payment_frequency: str = "monthly"
    remaining_balance: float = 0
    status: str = "active"
    created_at: Optional[datetime] = None


class AmortizationScheduleItem(BaseModel):
    period: int
    payment: float
    principal_component: float
    interest_component: float
    balance: float


class DebtSummary(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    total_debt: float
    total_interest: float
    total_monthly_payments: float
    debt_to_equity: float = 0
    weighted_avg_rate: float
    loans: List[Dict] = []
