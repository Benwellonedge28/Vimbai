"""
Due Diligence Service
Port: 8242
M&A due diligence analysis
"""

from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Due Diligence Service", version="1.0.0")


class DueDiligenceRequest(BaseModel):
    target_id: str
    acquirer_id: str
    financial_metrics: Dict[str, float]
    legal_issues: List[str]
    regulatory_flags: List[str]
    customer_concentration: float
    supplier_concentration: float
    key_person_risk: bool
    litigation_risk: float
    environmental_risk: bool
    cybersecurity_risk: str


class DueDiligenceResponse(BaseModel):
    target_id: str
    acquirer_id: str
    assessment_date: str
    financial_health: Dict[str, Any]
    operational_risk: Dict[str, Any]
    legal_compliance: Dict[str, Any]
    overall_risk_score: float
    risk_rating: str
    red_flags: List[str]
    green_flags: List[str]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "due-diligence", "version": "1.0.0"}


@app.post("/assess", response_model=DueDiligenceResponse)
async def assess_due_diligence(request: DueDiligenceRequest):
    logger.info("Performing due diligence", target=request.target_id, acquirer=request.acquirer_id)

    risk_score = 0
    red_flags = []
    green_flags = []

    fin = request.financial_metrics
    profit_margin = fin.get("net_income", 0) / fin.get("revenue", 1) if fin.get("revenue") else 0
    debt_ratio = fin.get("total_debt", 0) / fin.get("total_assets", 1) if fin.get("total_assets") else 0
    current_ratio = (
        fin.get("current_assets", 0) / fin.get("current_liabilities", 1) if fin.get("current_liabilities") else 0
    )
    roe = fin.get("net_income", 0) / fin.get("equity", 1) if fin.get("equity") else 0

    financial_health = {
        "profit_margin": round(profit_margin, 4),
        "debt_ratio": round(debt_ratio, 4),
        "current_ratio": round(current_ratio, 4),
        "roe": round(roe, 4),
        "revenue_growth": fin.get("revenue_growth", 0),
        "score": "Strong" if profit_margin > 0.1 and debt_ratio < 0.5 else "Weak",
    }

    if profit_margin < 0:
        risk_score += 20
        red_flags.append("Negative profit margin detected")
    else:
        green_flags.append("Profitable operations")

    if debt_ratio > 0.7:
        risk_score += 15
        red_flags.append("High leverage ratio")
    elif debt_ratio < 0.4:
        green_flags.append("Conservative capital structure")

    operational_risk = {
        "customer_concentration": request.customer_concentration,
        "supplier_concentration": request.supplier_concentration,
        "key_person_risk": request.key_person_risk,
        "cybersecurity_risk": request.cybersecurity_risk,
    }

    if request.customer_concentration > 0.3:
        risk_score += 10
        red_flags.append(f"High customer concentration: {request.customer_concentration*100:.1f}%")
    else:
        green_flags.append("Diversified customer base")

    if request.key_person_risk:
        risk_score += 10
        red_flags.append("Key person dependency identified")

    legal_compliance = {
        "legal_issues_count": len(request.legal_issues),
        "regulatory_flags_count": len(request.regulatory_flags),
        "litigation_risk": request.litigation_risk,
        "environmental_risk": request.environmental_risk,
    }

    if request.legal_issues:
        risk_score += len(request.legal_issues) * 5
        red_flags.append(f"{len(request.legal_issues)} legal issues identified")

    if request.regulatory_flags:
        risk_score += len(request.regulatory_flags) * 5
        red_flags.append(f"{len(request.regulatory_flags)} regulatory concerns")

    if request.environmental_risk:
        risk_score += 15
        red_flags.append("Environmental compliance risk")

    if request.litigation_risk > 0.5:
        risk_score += 10
        red_flags.append("High litigation exposure")

    risk_rating = "HIGH" if risk_score > 50 else "MEDIUM" if risk_score > 25 else "LOW"

    if risk_score < 20:
        green_flags.append("Low overall risk profile")

    recommendations = []
    if risk_score > 40:
        recommendations.append("Consider detailed investigation of flagged issues before proceeding")
    if request.customer_concentration > 0.3:
        recommendations.append("Develop customer diversification plan as part of integration")
    if request.key_person_risk:
        recommendations.append("Implement retention agreements and succession planning")

    return DueDiligenceResponse(
        target_id=request.target_id,
        acquirer_id=request.acquirer_id,
        assessment_date=datetime.now().isoformat(),
        financial_health=financial_health,
        operational_risk=operational_risk,
        legal_compliance=legal_compliance,
        overall_risk_score=risk_score,
        risk_rating=risk_rating,
        red_flags=red_flags,
        green_flags=green_flags,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8242)
