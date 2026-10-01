"""Pydantic models for the Tax Compliance Service."""

from enum import Enum
from typing import Dict, List

from pydantic import BaseModel, Field


class ComplianceStatus(str, Enum):
    COMPLIANT = "compliant"
    PENDING = "pending"
    OVERDUE = "overdue"
    FILED = "filed"


class TaxObligationCreate(BaseModel):
    company_id: str
    obligation_type: str  # vat_return, paye, income_tax, withholding, capital_gains
    description: str
    due_date: str
    amount: float = 0
    filing_frequency: str = "monthly"  # monthly, quarterly, annual


class TaxObligation(BaseModel):
    id: str
    company_id: str
    obligation_type: str
    description: str
    due_date: str
    amount: float = 0
    status: ComplianceStatus = ComplianceStatus.PENDING
    filing_frequency: str = "monthly"


class ComplianceSummary(BaseModel):
    company_id: str
    total_obligations: int
    compliant: int
    pending: int
    overdue: int
    filed: int
    compliance_score: float
    upcoming_deadlines: List[Dict] = []
    obligations: List[TaxObligation] = []
