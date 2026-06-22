"""
Audit Planning Service
Port: 8195
Audit strategy, materiality, risk assessment
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Audit Planning Service", version="1.0.0")

class AuditScope(BaseModel):
    entities: List[str]
    periods: List[str]
    accounts: List[str]
    locations: List[str]

class MaterialityLevels(BaseModel):
    planning_materiality: float
    performance_materiality: float
    thresholds_unadjusted: float

class RiskAssessment(BaseModel):
    inherent_risk: str
    control_risk: str
    detection_risk: float
    risk_level: str

class AuditPlanningRequest(BaseModel):
    audit_id: str
    company_id: str
    fiscal_year: str
    prior_year_findings: List[Dict[str, Any]]
    industry_risk_factors: List[str]
    regulatory_requirements: List[str]
    client_acceptance: bool

class AuditPlanningResponse(BaseModel):
    audit_id: str
    materiality: MaterialityLevels
    audit_scope: AuditScope
    risk_assessment: Dict[str, RiskAssessment]
    audit_strategy: Dict[str, str]
    resource_requirements: Dict[str, int]
    timeline: Dict[str, str]
    key_focus_areas: List[str]

async def call_internal_service(service_url: str, endpoint: str, data: dict = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            response = await client.post(url, json=data) if data else await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "audit-planning", "version": "1.0.0"}

@app.post("/plan", response_model=AuditPlanningResponse)
async def create_audit_plan(request: AuditPlanningRequest):
    logger.info("Creating audit plan", audit=request.audit_id, company=request.company_id)

    base_materiality = 1000000.0
    planning_materiality = base_materiality * 0.5
    performance_materiality = planning_materiality * 0.75

    risk_counts = {"high": 0, "medium": 0, "low": 0}
    for finding in request.prior_year_findings:
        risk = finding.get("risk_level", "medium")
        if risk in risk_counts:
            risk_counts[risk] += 1

    overall_risk = "high" if risk_counts["high"] > 3 else "medium" if risk_counts["medium"] > 2 else "low"

    return AuditPlanningResponse(
        audit_id=request.audit_id,
        materiality=MaterialityLevels(
            planning_materiality=round(planning_materiality, 2),
            performance_materiality=round(performance_materiality, 2),
            thresholds_unadjusted=round(performance_materiality * 0.05, 2)
        ),
        audit_scope=AuditScope(
            entities=[f"Entity_{i}" for i in range(1, 4)],
            periods=[request.fiscal_year],
            accounts=["Revenue", "Assets", "Liabilities", "Equity"],
            locations=["Head Office", "Regional Office 1"]
        ),
        risk_assessment={
            "revenue_recognition": RiskAssessment(
                inherent_risk="high",
                control_risk="medium",
                detection_risk=0.10,
                risk_level="high"
            ),
            "inventory": RiskAssessment(
                inherent_risk="medium",
                control_risk="low",
                detection_risk=0.05,
                risk_level="medium"
            )
        },
        audit_strategy={
            "approach": "Risk-based audit approach",
            "sampling_method": "Statistical sampling with random selection",
            "testing_strategy": "Substantive procedures for high-risk areas"
        },
        resource_requirements={
            "senior_auditors": 2,
            "junior_auditors": 4,
            "specialists": 1,
            "estimated_hours": 800
        },
        timeline={
            "planning_start": f"{request.fiscal_year}-01-01",
            "fieldwork_start": f"{request.fiscal_year}-03-01",
            "fieldwork_end": f"{request.fiscal_year}-05-31",
            "report_issue": f"{request.fiscal_year}-06-30"
        },
        key_focus_areas=["Revenue recognition", "Going concern assessment", "Related party transactions"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8195)
