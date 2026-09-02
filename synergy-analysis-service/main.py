"""
Synergy Analysis Service
Port: 8243
M&A synergy identification and valuation
"""

import math
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Synergy Analysis Service", version="1.0.0")


class SynergyItem(BaseModel):
    category: str
    description: str
    annual_value: float
    probability: float
    timing_months: int


class SynergyAnalysisRequest(BaseModel):
    deal_id: str
    acquirer_id: str
    target_id: str
    synergies: List[SynergyItem]
    integration_costs: float
    cost_of_capital: float


class SynergyAnalysisResponse(BaseModel):
    deal_id: str
    synergy_summary: Dict[str, Any]
    pv_synergies: float
    pv_integration_costs: float
    net_synergy_value: float
    irr: float
    payback_months: float
    risk_adjusted_value: float
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "synergy-analysis", "version": "1.0.0"}


@app.post("/analyze", response_model=SynergyAnalysisResponse)
async def analyze_synergies(request: SynergyAnalysisRequest):
    logger.info("Analyzing synergies", deal=request.deal_id)

    cost_revenue, cost_cost, cost_operational, cost_other = 0, 0, 0, 0

    for s in request.synergies:
        if s.category == "revenue":
            cost_revenue += s.annual_value * s.probability
        elif s.category == "cost":
            cost_cost += s.annual_value * s.probability
        elif s.category == "operational":
            cost_operational += s.annual_value * s.probability
        else:
            cost_other += s.annual_value * s.probability

    total_annual = cost_revenue + cost_cost + cost_operational + cost_other
    pv_synergies = sum(
        s.annual_value * s.probability / ((1 + request.cost_of_capital) ** (s.timing_months / 12))
        for s in request.synergies
    )

    total_integration = request.integration_costs * 1.1
    net_value = pv_synergies - total_integration

    monthly_benefit = total_annual / 12
    payback_months = total_integration / monthly_benefit if monthly_benefit else 0

    irr_3yr = ((pv_synergies / total_integration) ** (1 / 3) - 1) * 100 if total_integration else 0

    risk_adjusted = pv_synergies * 0.75

    synergy_summary = {
        "revenue_synergies": round(cost_revenue, 2),
        "cost_synergies": round(cost_cost, 2),
        "operational_synergies": round(cost_operational, 2),
        "other_synergies": round(cost_other, 2),
        "total_annual_synergies": round(total_annual, 2),
    }

    recommendations = []
    if total_annual < request.integration_costs * 0.2:
        recommendations.append("Synergy case appears weak relative to integration costs")
    if payback_months > 36:
        recommendations.append("Long payback period - ensure board alignment on timeline")
    if cost_revenue < total_annual * 0.2:
        recommendations.append("Limited revenue synergies - ensure cost synergies are achievable")
    if net_value < 0:
        recommendations.append("WARNING: Negative net synergy value - reconsider deal economics")

    return SynergyAnalysisResponse(
        deal_id=request.deal_id,
        synergy_summary=synergy_summary,
        pv_synergies=round(pv_synergies, 2),
        pv_integration_costs=round(total_integration, 2),
        net_synergy_value=round(net_value, 2),
        irr=round(irr_3yr, 2),
        payback_months=round(payback_months, 2),
        risk_adjusted_value=round(risk_adjusted, 2),
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8243)
