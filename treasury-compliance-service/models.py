"""Pydantic models for the Treasury Compliance Service."""

import uuid
from datetime import datetime, timezone
from enum import Enum

from pydantic import BaseModel, Field


class ComplianceStatus(str, Enum):
    COMPLIANT = "compliant"
    WARNING = "warning"
    NON_COMPLIANT = "non_compliant"
    PENDING_REVIEW = "pending_review"


class ComplianceCheck(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    check_name: str
    regulation: str
    status: ComplianceStatus = ComplianceStatus.PENDING_REVIEW
    details: str = ""
    checked_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    remediation: str = ""


DEFAULT_CHECKS = [
    {
        "check_name": "Counterparty Limit Compliance",
        "regulation": "Basel III",
        "description": "Ensure counterparty exposure is within regulatory limits",
    },
    {"check_name": "Liquidity Coverage Ratio", "regulation": "Basel III LCR", "description": "Maintain LCR above 100%"},
    {
        "check_name": "FX Exposure Limits",
        "regulation": "Internal Policy",
        "description": "Verify foreign exchange exposure within approved limits",
    },
    {
        "check_name": "Investment Guidelines",
        "regulation": "Board Policy",
        "description": "Ensure investments comply with board-approved guidelines",
    },
    {
        "check_name": "Segregation of Duties",
        "regulation": "SOX",
        "description": "Verify treasury duties are properly segregated",
    },
    {
        "check_name": "Reporting Timeliness",
        "regulation": "Regulatory",
        "description": "Ensure regulatory reports submitted on time",
    },
]
