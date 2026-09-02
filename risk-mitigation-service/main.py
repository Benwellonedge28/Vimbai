"""Vimbai Risk Mitigation Service - Risk management and investigation. Port: 8331"""

import os
import uuid
from collections import defaultdict
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "risk-mitigation-service"
PORT = int(os.getenv("PORT", "8331"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Risk Mitigation Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="risk-mitigation-service", instrument_app=app)
except ImportError:
    TRACER = None


class RiskCategory(str, Enum):
    FINANCIAL = "financial"
    OPERATIONAL = "operational"
    STRATEGIC = "strategic"
    COMPLIANCE = "compliance"
    CYBER = "cyber"
    MARKET = "market"
    CREDIT = "credit"
    LIQUIDITY = "liquidity"


class RiskLevel(str, Enum):
    LOW = "low"
    MODERATE = "moderate"
    HIGH = "high"
    CRITICAL = "critical"


class RiskItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    category: RiskCategory
    name: str
    description: str = ""
    likelihood: int = 1  # 1-5
    impact: int = 1  # 1-5
    risk_score: float = 0  # likelihood * impact
    level: RiskLevel = RiskLevel.LOW
    owner: str = ""
    mitigation: str = ""
    status: str = "identified"  # identified, assessing, mitigating, monitoring, closed
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


def calc_level(score: float) -> RiskLevel:
    if score <= 4:
        return RiskLevel.LOW
    if score <= 9:
        return RiskLevel.MODERATE
    if score <= 16:
        return RiskLevel.HIGH
    return RiskLevel.CRITICAL


_risks: Dict[str, List[RiskItem]] = defaultdict(list)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/risks")
async def create_risk(risk: RiskItem):
    risk.risk_score = risk.likelihood * risk.impact
    risk.level = calc_level(risk.risk_score)
    _risks[risk.company_id].append(risk)
    logger.info("risk_created", company_id=risk.company_id, category=risk.category, level=risk.level.value)
    return {"id": risk.id, "risk_score": risk.risk_score, "level": risk.level.value}


@app.get("/risks/{company_id}")
async def get_risks(company_id: str, category: Optional[str] = None, level: Optional[str] = None):
    risks = _risks.get(company_id, [])
    if category:
        risks = [r for r in risks if r.category.value == category]
    if level:
        risks = [r for r in risks if r.level.value == level]
    return {
        "company_id": company_id,
        "risks": risks,
        "total": len(risks),
        "by_level": {l: sum(1 for r in risks if r.level.value == l) for l in RiskLevel},
    }


@app.put("/risks/{risk_id}")
async def update_risk(
    risk_id: str,
    likelihood: Optional[int] = None,
    impact: Optional[int] = None,
    mitigation: Optional[str] = None,
    status: Optional[str] = None,
):
    for risks in _risks.values():
        for r in risks:
            if r.id == risk_id:
                if likelihood is not None:
                    r.likelihood = likelihood
                if impact is not None:
                    r.impact = impact
                if mitigation is not None:
                    r.mitigation = mitigation
                if status is not None:
                    r.status = status
                r.risk_score = r.likelihood * r.impact
                r.level = calc_level(r.risk_score)
                r.updated_at = datetime.now(timezone.utc)
                return {"id": r.id, "risk_score": r.risk_score, "level": r.level.value, "status": r.status}
    raise HTTPException(status_code=404, detail="Risk not found")


@app.get("/dashboard/{company_id}")
async def risk_dashboard(company_id: str):
    risks = _risks.get(company_id, [])
    if not risks:
        return {
            "company_id": company_id,
            "total_risks": 0,
            "by_level": {},
            "by_category": {},
            "avg_score": 0,
            "top_risks": [],
        }
    by_level = {l.value: sum(1 for r in risks if r.level.value == l.value) for l in RiskLevel}
    by_category = {c.value: sum(1 for r in risks if r.category.value == c.value) for c in RiskCategory}
    avg = sum(r.risk_score for r in risks) / len(risks)
    top = sorted(risks, key=lambda r: r.risk_score, reverse=True)[:5]
    return {
        "company_id": company_id,
        "total_risks": len(risks),
        "by_level": by_level,
        "by_category": by_category,
        "avg_score": avg,
        "top_risks": top,
    }


@app.delete("/risks/{risk_id}")
async def close_risk(risk_id: str):
    for risks in _risks.values():
        for r in risks:
            if r.id == risk_id:
                r.status = "closed"
                return {"id": risk_id, "status": "closed"}
    raise HTTPException(status_code=404, detail="Risk not found")


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
