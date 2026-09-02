"""
Treasury Risk Service
Port: 8259
Treasury risk assessment and monitoring
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Treasury Risk Service", version="1.0.0")


class TreasuryExposure(BaseModel):
    exposure_type: str
    amount: float
    currency: str
    risk_metric: float


class TreasuryRiskRequest(BaseModel):
    company_id: str
    exposures: List[TreasuryExposure]
    total_cash: float
    total_debt: float
    var_confidence: float


class TreasuryRiskResponse(BaseModel):
    company_id: str
    risk_summary: Dict[str, Any]
    exposure_analysis: List[Dict[str, Any]]
    stress_test_results: List[Dict[str, Any]]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "treasury-risk", "version": "1.0.0"}


@app.post("/analyze", response_model=TreasuryRiskResponse)
async def analyze_treasury_risk(request: TreasuryRiskRequest):
    logger.info("Analyzing treasury risk", company=request.company_id)

    total_exposure = sum(e.amount for e in request.exposures)

    exposure_analysis = []
    for exp in request.exposures:
        exposure_analysis.append(
            {
                "type": exp.exposure_type,
                "amount": round(exp.amount, 2),
                "currency": exp.currency,
                "pct_total": round(exp.amount / total_exposure * 100, 2) if total_exposure else 0,
            }
        )

    var_99 = total_exposure * request.var_confidence * 2.33
    var_95 = total_exposure * request.var_confidence * 1.65

    stress_scenarios = [
        {"scenario": "Market Crash -30%", "impact": round(-total_exposure * 0.3, 2)},
        {"scenario": "Interest Rate +200bp", "impact": round(-request.total_debt * 0.02, 2)},
        {"scenario": "FX Move +10%", "impact": round(-total_exposure * 0.1, 2)},
    ]

    risk_summary = {
        "total_exposure": round(total_exposure, 2),
        "total_cash": request.total_cash,
        "total_debt": request.total_debt,
        "net_exposure": round(total_exposure - request.total_cash, 2),
        "var_95": round(var_95, 2),
        "var_99": round(var_99, 2),
    }

    recommendations = []
    if var_99 > request.total_cash * 0.3:
        recommendations.append("High VaR relative to cash - increase liquidity buffer")
    if risk_summary["net_exposure"] > 0:
        recommendations.append("Net exposure positive - consider hedging strategies")

    return TreasuryRiskResponse(
        company_id=request.company_id,
        risk_summary=risk_summary,
        exposure_analysis=exposure_analysis,
        stress_test_results=stress_scenarios,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8259)
