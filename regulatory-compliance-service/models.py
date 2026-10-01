"""Pydantic models for the Regulatory Compliance Service."""

import uuid
from enum import Enum
from typing import Dict, List

from pydantic import BaseModel, Field


class RegStatus(str, Enum):
    COMPLIANT = "compliant"
    NON_COMPLIANT = "non_compliant"
    PENDING_REVIEW = "pending_review"
    NOT_APPLICABLE = "n/a"


class RegulationCreate(BaseModel):
    company_id: str
    regulation_name: str
    jurisdiction: str
    framework: str  # IFRS, IAS, SOX, Basel III, AML, GDPR, PCI_DSS
    requirement: str
    status: RegStatus = RegStatus.PENDING_REVIEW
    last_reviewed: str = ""
    next_review_due: str = ""
    risk_if_non_compliant: str = "medium"  # low, medium, high, critical


class Regulation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    regulation_name: str
    jurisdiction: str
    framework: str
    requirement: str
    status: RegStatus = RegStatus.PENDING_REVIEW
    last_reviewed: str = ""
    next_review_due: str = ""
    risk_if_non_compliant: str = "medium"


class ComplianceDashboard(BaseModel):
    company_id: str
    total_regulations: int
    compliant: int
    non_compliant: int
    pending: int
    compliance_rate: float
    by_framework: Dict[str, Dict] = {}
    by_jurisdiction: Dict[str, Dict] = []
    critical_items: List[Dict] = []
