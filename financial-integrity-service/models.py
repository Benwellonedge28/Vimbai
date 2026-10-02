"""Pydantic models for the Financial Integrity Service."""

import uuid
from datetime import datetime, timezone
from typing import List

from pydantic import BaseModel, Field


class IntegrityCheck(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    check_type: str  # balance_check, hash_verify, reconciliation, completeness
    entity_type: str = ""
    entity_id: str = ""
    passed: bool = False
    details: str = ""
    hash_before: str = ""
    hash_after: str = ""
    checked_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class IntegrityReport(BaseModel):
    company_id: str
    total_checks: int = 0
    passed: int = 0
    failed: int = 0
    pass_rate: float = 0
    checks: List[IntegrityCheck] = []
