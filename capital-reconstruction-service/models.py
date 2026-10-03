"""Pydantic models for Capital Reconstruction Service (API contract unchanged)."""

import uuid
from datetime import datetime
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


class ReconstructionType(str, Enum):
    SIMPLIFICATION = "simplification"
    FINANCIAL_RESTRUCTURING = "financial_restructuring"
    WRITE_OFF_EXCESS_CAPITAL = "write_off_excess_capital"
    CONSOLIDATION = "consolidation"
    SUBSTITUTION = "substitution"


class CapitalReconstruction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    reconstruction_type: str
    description: str
    scheme_date: datetime
    court_approval_date: Optional[datetime] = None
    shareholders_approval_date: Optional[datetime] = None
    previous_share_capital: float = 0
    new_share_capital: float = 0
    capital_reduction_amount: float = 0
    share_consolidation_ratio: str = ""  # e.g., "2:1"
    journal_entry_id: Optional[str] = None
    status: str = "draft"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ReconstructionAdjustment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    reconstruction_id: str
    account_code: str
    account_name: str
    previous_balance: float
    adjustment_type: str  # write_off, transfer, consolidate
    adjustment_amount: float
    new_balance: float = 0
    description: str
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ReserveConversion(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    reconstruction_id: str
    from_account: str
    from_account_name: str
    to_account: str
    to_account_name: str
    amount: float
    conversion_date: datetime
    reason: str
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
