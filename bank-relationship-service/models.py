"""Pydantic models for the Bank Relationship Service."""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


class RelationshipStatus(str, Enum):
    ACTIVE = "active"
    DORMANT = "dormant"
    TERMINATED = "terminated"
    PROSPECTIVE = "prospective"


class BankRelationship(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    bank_name: str
    branch: str = ""
    account_number: str = ""
    relationship_manager: str = ""
    contact_email: str = ""
    contact_phone: str = ""
    services: List[str] = []  # e.g., ["checking", "credit_line", "fx", "trade_finance"]
    status: RelationshipStatus = RelationshipStatus.ACTIVE
    opened_date: Optional[datetime] = None
    rating: int = 3  # 1-5
    notes: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class BankRelationshipCreate(BaseModel):
    company_id: str
    bank_name: str
    branch: str = ""
    account_number: str = ""
    relationship_manager: str = ""
    contact_email: str = ""
    contact_phone: str = ""
    services: List[str] = []
    opened_date: Optional[datetime] = None
    rating: int = 3
    notes: str = ""


class ServiceQualityMetric(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    relationship_id: str
    metric_name: str  # e.g., "response_time", "fee_competitiveness", "online_banking_quality"
    score: int = 1  # 1-5
    notes: str = ""
    recorded_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ServiceQualityMetricCreate(BaseModel):
    relationship_id: str
    metric_name: str
    score: int = 1
    notes: str = ""
