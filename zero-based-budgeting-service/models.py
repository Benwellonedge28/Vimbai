"""Pydantic models for the Zero-Based Budgeting Service."""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


class ZBBStatus(str, Enum):
    DRAFT = "draft"
    SUBMITTED = "submitted"
    REVIEWED = "reviewed"
    APPROVED = "approved"
    REJECTED = "rejected"


class BudgetItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    department: str
    cost_center: str = ""
    category: str
    description: str
    amount: float
    justification: str
    priority: int = 3  # 1 (highest) to 5 (lowest)
    alternative_options: str = ""
    impact_if_cut: str = ""
    status: str = "pending"


class ZBBPackage(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    period: str  # e.g., "2026-Q1"
    name: str
    department: str
    items: List[BudgetItem] = []
    total_amount: float = 0
    status: ZBBStatus = ZBBStatus.DRAFT
    reviewer: str = ""
    review_notes: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ZBBPackageCreate(BaseModel):
    company_id: str
    period: str
    name: str
    department: str
    items: List[BudgetItem] = []
