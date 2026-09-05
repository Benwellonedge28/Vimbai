"""Pydantic models for the Risk Reporting Service."""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Optional

from pydantic import BaseModel, Field


class RiskCategory(str, Enum):
    FINANCIAL = "financial"
    OPERATIONAL = "operational"
    STRATEGIC = "strategic"
    COMPLIANCE = "compliance"
    CYBER = "cyber"
    MARKET = "market"
    CREDIT = "credit"
    LIQUIDITY = "liquidity"


class RiskLevel(str, Enum):
    LOW = "low"
    MODERATE = "moderate"
    HIGH = "high"
    CRITICAL = "critical"


def calc_level(score: float) -> RiskLevel:
    if score <= 4:
        return RiskLevel.LOW
    if score <= 9:
        return RiskLevel.MODERATE
    if score <= 16:
        return RiskLevel.HIGH
    return RiskLevel.CRITICAL


class RiskItemBase(BaseModel):
    company_id: str
    category: RiskCategory
    name: str
    description: str = ""
    likelihood: int = Field(default=1, ge=1, le=5)
    impact: int = Field(default=1, ge=1, le=5)
    owner: str = ""
    mitigation: str = ""


class RiskItemCreate(RiskItemBase):
    """Payload for creating a risk item."""


class RiskItem(RiskItemBase):
    """Risk record as stored / returned by the API."""

    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    risk_score: float = 0
    level: RiskLevel = RiskLevel.LOW
    status: str = "identified"  # identified, assessing, mitigating, monitoring, closed
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
