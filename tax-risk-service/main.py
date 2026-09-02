"""
Vimbai Tax Risk Service
Tax risk assessment, exposure quantification, and mitigation recommendations.
Port: 8373
"""

import os
import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "tax-risk-service"
PORT = int(os.getenv("PORT", "8373"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Tax Risk Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class RiskLevel(str, Enum):
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"


class TaxRiskItem(BaseModel):
    risk_id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    description: str
    risk_type: str  # transfer_pricing, vat_gap, permanent_establishment, treaty, reporting
    potential_exposure: float
    probability: float = 0.5
    jurisdiction: str = "ZW"
    mitigation_actions: List[str] = []


class TaxRiskRequest(BaseModel):
    company_id: str
    risks: List[TaxRiskItem]


class TaxRiskAssessment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    assessment_date: str
    overall_risk_level: RiskLevel
    total_exposure: float
    weighted_exposure: float
    risk_count: int
    risks: List[Dict]
    recommendations: List[str] = []


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/assess", response_model=TaxRiskAssessment)
async def assess_risk(req: TaxRiskRequest):
    total_exposure = sum(r.potential_exposure for r in req.risks)
    weighted = sum(r.potential_exposure * r.probability for r in req.risks)

    if weighted > 500000:
        overall = RiskLevel.CRITICAL
    elif weighted > 100000:
        overall = RiskLevel.HIGH
    elif weighted > 25000:
        overall = RiskLevel.MEDIUM
    else:
        overall = RiskLevel.LOW

    risks_detail = []
    for r in req.risks:
        risk_score = r.potential_exposure * r.probability
        level = (
            RiskLevel.CRITICAL
            if risk_score > 250000
            else RiskLevel.HIGH if risk_score > 50000 else RiskLevel.MEDIUM if risk_score > 10000 else RiskLevel.LOW
        )
        risks_detail.append(
            {
                "risk_id": r.risk_id,
                "description": r.description,
                "risk_type": r.risk_type,
                "potential_exposure": r.potential_exposure,
                "probability": r.probability,
                "weighted_score": round(risk_score, 2),
                "level": level.value,
                "jurisdiction": r.jurisdiction,
                "mitigation_actions": r.mitigation_actions,
            }
        )

    recommendations = [
        "Conduct regular transfer pricing documentation reviews",
        "Implement real-time VAT reconciliation to minimize VAT gaps",
        "Monitor permanent establishment risks in cross-border operations",
        "Maintain up-to-date tax treaty position documentation",
        "Engage external tax advisors for high-exposure areas",
    ]

    return TaxRiskAssessment(
        company_id=req.company_id,
        assessment_date=datetime.now(timezone.utc).isoformat(),
        overall_risk_level=overall,
        total_exposure=round(total_exposure, 2),
        weighted_exposure=round(weighted, 2),
        risk_count=len(req.risks),
        risks=risks_detail,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
