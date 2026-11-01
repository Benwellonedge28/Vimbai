"""Pydantic models for the Treasury Analytics Service."""

from decimal import Decimal
from typing import List

from pydantic import BaseModel


class TreasuryKPI(BaseModel):
    name: str
    value: Decimal
    unit: str
    benchmark: Decimal = Decimal("0")
    status: str = "good"
    description: str = ""


class AnalyticsRequest(BaseModel):
    company_id: str
    total_cash: Decimal = Decimal("0")
    monthly_inflow: Decimal = Decimal("0")
    monthly_outflow: Decimal = Decimal("0")
    short_term_debt: Decimal = Decimal("0")
    total_debt: Decimal = Decimal("0")
    investments: Decimal = Decimal("0")
    fx_exposure: Decimal = Decimal("0")


class AnalyticsResponse(BaseModel):
    company_id: str
    kpis: List[TreasuryKPI]
    cash_adequacy_days: Decimal
    debt_service_ratio: Decimal
    investment_yield: Decimal
    fx_risk_score: Decimal
