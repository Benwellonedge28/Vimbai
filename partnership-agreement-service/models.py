"""Pydantic models for Partnership Agreement Service (API contract unchanged)."""

import uuid
from datetime import datetime
from typing import List, Optional

from pydantic import BaseModel, Field


class Partner(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    address: str
    contribution: float
    profit_sharing_ratio: float
    is_active: bool = True


"""Pydantic models for Partnership Agreement Service (API contract unchanged).
"""

import uuid
from datetime import datetime
from typing import List, Optional

from pydantic import BaseModel, Field


class PartnershipAgreement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    agreement_number: str
    partnership_name: str
    partners: List[Partner] = []
    start_date: datetime
    end_date: Optional[datetime] = None
    duration_years: Optional[int] = None
    business_nature: str
    capital_amount: float = 0
    profit_sharing_basis: str = "ratio"  # equal, ratio, capital_based
    drawings_allowed: bool = True
    max_drawings: Optional[float] = None
    interest_on_capital_rate: float = 0
    interest_on_drawings_rate: float = 0
    guaranteed_salary: bool = False
    commission_allowed: bool = False
    admission_new_partner: bool = True
    retirement_conditions: str = ""
    dissolution_conditions: str = ""
    dispute_resolution: str = ""
    is_active: bool = True
    agreement_document: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
