"""Pydantic models for the Revenue Recognition Service."""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


class RecognitionMethod(str, Enum):
    POINT_IN_TIME = "point_in_time"
    OVER_TIME = "over_time"


class PerformanceObligation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    description: str
    transaction_price: float
    standalone_selling_price: float = 0
    recognition_method: RecognitionMethod = RecognitionMethod.POINT_IN_TIME
    is_satisfied: bool = False
    revenue_recognized: float = 0


class PerformanceObligationCreate(BaseModel):
    description: str
    transaction_price: float
    standalone_selling_price: float = 0
    recognition_method: RecognitionMethod = RecognitionMethod.POINT_IN_TIME


class RevenueContractCreate(BaseModel):
    company_id: str
    customer_name: str
    contract_date: Optional[datetime] = None
    obligations: List[PerformanceObligationCreate] = []


class RevenueContract(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    customer_name: str
    contract_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    total_transaction_price: float = 0
    obligations: List[PerformanceObligation] = []
    total_revenue_recognized: float = 0
    deferred_revenue: float = 0
    status: str = "active"
