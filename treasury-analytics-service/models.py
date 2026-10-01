"""Pydantic models for the Treasury Analytics Service."""

from typing import List

from pydantic import BaseModel


class TreasuryKPI(BaseModel):
    name: str
    value: float
    unit: str
    benchmark: float = 0
    status: str = "good"
    description: str = ""


class AnalyticsRequest(BaseModel):
    company_id: str
    total_cash: float = 0
    monthly_inflow: float = 0
    monthly_outflow: float = 0
    short_term_debt: float = 0
    total_debt: float = 0
    investments: float = 0
    fx_exposure: float = 0


class AnalyticsResponse(BaseModel):
    company_id: str
    kpis: List[TreasuryKPI]
    cash_adequacy_days: float
    debt_service_ratio: float
    investment_yield: float
    fx_risk_score: float
