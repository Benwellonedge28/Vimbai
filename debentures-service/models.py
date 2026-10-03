"""Pydantic models for Debentures Service (API contract unchanged)."""

import uuid
from datetime import datetime
from typing import Optional

from pydantic import BaseModel, Field


class DebentureClass(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    company_id: str
    nominal_value: float
    issue_price: float
    coupon_rate: float  # Annual interest rate as percentage
    interest_payment_frequency: str  # annual, semi_annual, quarterly, monthly
    maturity_date: datetime
    redemption_price: float
    convertibility: str = "none"  # none, convertible, optionally_convertible
    conversion_terms: Optional[str] = None
    debentures_issued: int = 0
    debentures_outstanding: int = 0


class DebentureIssue(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    debenture_class_id: str
    debentures_issued: int
    issue_date: datetime
    total_proceeds: float = 0
    discount_on_issue: float = 0
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class InterestPayment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    debenture_class_id: str
    period_start: datetime
    period_end: datetime
    debentures_outstanding: int
    interest_rate: float
    interest_amount: float = 0
    tax_deducted: float = 0
    net_payment: float = 0
    payment_date: Optional[datetime] = None
    journal_entry_id: Optional[str] = None
    status: str = "accrued"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class RedemptionEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    debenture_class_id: str
    debentures_redeemed: int
    redemption_date: datetime
    redemption_price: float
    total_proceeds: float = 0
    premium_on_redemption: float = 0
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
