"""Pydantic models for the Right Issues Service.

:RightIssue via :OWNS_RIGHT_ISSUE and :Renunciation via :OWNS_RENUNCIATION, both
Book-gated. The entitlement, acceptance and renunciation maps are kept as JSON
strings on the node; Neo4j has no map property.
"""

import uuid
from datetime import datetime, timezone
from typing import Dict, Optional

from pydantic import BaseModel, Field


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class RightIssue(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    issue_date: datetime
    expiry_date: datetime
    share_class: str
    rights_offered: int  # rights per existing share
    new_shares_offered: int
    issue_price: float
    nominal_value: float
    total_proceeds: float = 0
    renouncement_allowed: bool = True
    entitlements: Dict[str, int] = {}  # shareholder_id -> rights entitlement
    acceptances: Dict[str, int] = {}  # shareholder_id -> shares accepted
    renunciations: Dict[str, str] = {}  # shareholder_id -> new_owner_id
    journal_entry_id: Optional[str] = None
    status: str = "offered"
    created_at: datetime = Field(default_factory=_utcnow)


class Renunciation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    right_issue_id: str
    original_shareholder_id: str
    new_shareholder_id: str
    renunciation_date: datetime
    renunciation_price: float
