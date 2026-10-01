"""Pydantic models for the Financial Identity Service."""

import uuid
from datetime import datetime
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


class VerificationStatus(str, Enum):
    PENDING = "pending"
    VERIFIED = "verified"
    FAILED = "failed"
    EXPIRED = "expired"


class FinancialProfile(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: str  # the KYC subject (the person the profile describes)
    legal_name: str
    national_id: str = ""
    tax_id: str = ""
    date_of_birth: Optional[datetime] = None
    address: str = ""
    phone: str = ""
    email: str = ""
    employer: str = ""
    annual_income: float = 0
    verification_status: VerificationStatus = VerificationStatus.PENDING
    verified_at: Optional[datetime] = None
    risk_score: int = 0
    kyc_documents: List[str] = []


class FinancialProfileCreate(BaseModel):
    user_id: str
    legal_name: str
    national_id: str = ""
    tax_id: str = ""
    date_of_birth: Optional[datetime] = None
    address: str = ""
    phone: str = ""
    email: str = ""
    employer: str = ""
    annual_income: float = 0
