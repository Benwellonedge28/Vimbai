"""Pydantic models for the Forensic Accounting Service."""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


class AuditStatus(str, Enum):
    PLANNED = "planned"
    IN_PROGRESS = "in_progress"
    COMPLETED = "completed"
    CANCELLED = "cancelled"


class FindingSeverity(str, Enum):
    INFO = "info"
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"


class AuditFinding(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    title: str
    description: str
    severity: FindingSeverity = FindingSeverity.MEDIUM
    recommendation: str = ""
    status: str = "open"  # open, remediated, accepted
    evidence: str = ""


class AuditFindingCreate(BaseModel):
    title: str
    description: str
    severity: FindingSeverity = FindingSeverity.MEDIUM
    recommendation: str = ""
    evidence: str = ""


class AuditEngagementCreate(BaseModel):
    company_id: str
    audit_type: str = "operational"
    title: str
    scope: str = ""
    objectives: List[str] = []
    start_date: Optional[datetime] = None
    end_date: Optional[datetime] = None
    auditor: str = ""


class AuditEngagement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    audit_type: str = "operational"
    title: str
    scope: str = ""
    objectives: List[str] = []
    start_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    end_date: Optional[datetime] = None
    auditor: str = ""
    status: AuditStatus = AuditStatus.PLANNED
    findings: List[AuditFinding] = []
    summary: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
