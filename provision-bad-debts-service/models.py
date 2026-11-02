"""Pydantic models for the Bad Debts Provision Service.

Policies, the debtors being provided against, and the journals a period produces
are nodes. The ageing pass writes back to each debtor, so what a calculation
worked out survives the next restart instead of being lost with the process.
"""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Dict, List, Optional

from pydantic import BaseModel, Field


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class ProvisionMethod(str, Enum):
    PERCENTAGE_OF_SALES = "percentage_of_sales"
    PERCENTAGE_OF_DEBTORS = "percentage_of_debtors"
    AGEING_ANALYSIS = "ageing_analysis"
    SPECIFIC_DEBTOR_PROVISION = "specific_debtor_provision"


class DebtorStatus(str, Enum):
    CURRENT = "current"
    OVERDUE_30 = "overdue_30"
    OVERDUE_60 = "overdue_60"
    OVERDUE_90 = "overdue_90"
    OVERDUE_180 = "overdue_180"
    DOUBTFUL = "doubtful"
    BAD = "bad"


class ProvisionEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    debtor_id: str
    debtor_name: str
    invoice_number: str
    original_amount: float
    outstanding_amount: float
    days_outstanding: int = 0
    status: DebtorStatus
    provision_percent: float = 0
    provision_amount: float = 0
    created_at: datetime = Field(default_factory=_utcnow)


class ProvisionPolicy(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    name: str
    method: ProvisionMethod
    policy_rules: Dict[str, float] = {}
    default_provision_percent: float = 0
    is_active: bool = True
    created_at: datetime = Field(default_factory=_utcnow)


class ProvisionJournal(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    period_end: datetime
    total_debtors: float = 0
    provision_opening: float = 0
    provision_required: float = 0
    provision_adjustment: float = 0
    provision_closing: float = 0
    entries: List[ProvisionEntry] = []
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=_utcnow)
