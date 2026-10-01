"""Pydantic models for the Insurance Claims Service."""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


class ClaimStatus(str, Enum):
    FILED = "filed"
    UNDER_REVIEW = "under_review"
    APPROVED = "approved"
    DENIED = "denied"
    PAID = "paid"


class InsuranceClaimCreate(BaseModel):
    company_id: str
    policy_number: str
    claim_type: str  # property, liability, auto, health, business_interruption
    incident_date: str
    claim_amount: float
    deductible: float = 0
    description: str = ""
    supporting_docs: List[str] = []
    coverage_limit: float = 0


class InsuranceClaim(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    policy_number: str
    claim_type: str
    incident_date: str
    claim_date: str = Field(default_factory=lambda: datetime.now(timezone.utc).strftime("%Y-%m-%d"))
    claim_amount: float
    deductible: float = 0
    description: str = ""
    supporting_docs: List[str] = []
    coverage_limit: float = 0
    status: ClaimStatus = ClaimStatus.FILED


class ClaimResult(BaseModel):
    claim_id: str
    company_id: str
    status: ClaimStatus
    covered_amount: float
    deductible_applied: float
    settlement_amount: float
    coverage_ratio: float
    notes: str = ""
