"""Pydantic models for Partnership Changes Service (API contract unchanged)."""

import uuid
from datetime import datetime
from enum import Enum
from typing import Dict, List, Optional

from pydantic import BaseModel, Field


class ChangeType(str, Enum):
    ADMISSION = "admission"
    RETIREMENT = "retirement"
    DEATH = "death"
    INSOLVENCY = "insolvency"
    EXPULSION = "expulsion"


class PartnerChange(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    partnership_id: str
    change_type: ChangeType
    partner_id: str
    partner_name: str
    effective_date: datetime
    capital_balance: float = 0
    current_account_balance: float = 0
    total_payable: float = 0
    goodwill_amount: float = 0
    payment_method: str = "cash"  # cash, assets, mixed
    settlement_status: str = "pending"
    journal_entry_id: Optional[str] = None
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class AdmissionDetails(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    new_partner_id: str
    new_partner_name: str
    capital_contribution: float
    goodwill_paid: float = 0
    premium_distribution: Dict[str, float] = {}  # partner_id -> amount
    new_profit_sharing_ratios: Dict[str, float] = {}
    revaluation_required: bool = False
    revaluation_amount: float = 0
    admission_date: datetime
    journal_entry_ids: List[str] = []
