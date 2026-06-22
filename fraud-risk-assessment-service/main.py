"""
Fraud Risk Assessment Service
Port: 8199
Fraud triangle analysis, risk indicators
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Fraud Risk Assessment Service", version="1.0.0")

class FraudIndicator(BaseModel):
    category: str
    indicator: str
    risk_level: str
    weight: float

class FraudRiskAssessmentRequest(BaseModel):
    audit_id: str
    company_id: str
    industry: str
    company_size: str
    financial_indicators: Dict[str, float]
    management_characteristics: List[str]
    internal_control_gaps: List[str]
    prior_fraud_history: bool

class FraudRiskAssessmentResponse(BaseModel):
    audit_id: str
    fraud_risk_score: float
    risk_category: str
    pressure_indicators: List[FraudIndicator]
    opportunity_indicators: List[FraudIndicator]
    rationalization_indicators: List[FraudIndicator]
    high_risk_areas: List[str]
    recommended_procedures: List[str]
    conclusion: str

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
    return {"status": "healthy", "service": "fraud-risk-assessment", "version": "1.0.0"}

@app.post("/assess", response_model=FraudRiskAssessmentResponse)
async def assess_fraud_risk(request: FraudRiskAssessmentRequest):
    logger.info("Assessing fraud risk", audit=request.audit_id, company=request.company_id)

    pressure_indicators = []
    opportunity_indicators = []
    rationalization_indicators = []

    pressure_score = 0.0
    opportunity_score = 0.0
    rationalization_score = 0.0

    if request.prior_fraud_history:
        pressure_indicators.append(FraudIndicator(category="pressure", indicator="Prior fraud incidents", risk_level="high", weight=0.3))
        pressure_score += 0.3

    if request.company_size == "small":
        pressure_indicators.append(FraudIndicator(category="pressure", indicator="Limited resources", risk_level="medium", weight=0.2))
        pressure_score += 0.2

    if "high_turnover" in request.management_characteristics:
        pressure_indicators.append(FraudIndicator(category="pressure", indicator="Management turnover", risk_level="high", weight=0.25))
        pressure_score += 0.25

    if len(request.internal_control_gaps) > 3:
        opportunity_indicators.append(FraudIndicator(category="opportunity", indicator="Multiple control deficiencies", risk_level="high", weight=0.35))
        opportunity_score += 0.35

    if "segregation_of_duties" in request.internal_control_gaps:
        opportunity_indicators.append(FraudIndicator(category="opportunity", indicator="Inadequate segregation", risk_level="high", weight=0.3))
        opportunity_score += 0.3

    if "aggressive_targets" in request.management_characteristics:
        rationalization_indicators.append(FraudIndicator(category="rationalization", indicator="Aggressive targets", risk_level="medium", weight=0.2))
        rationalization_score += 0.2

    total_score = (pressure_score * 0.4 + opportunity_score * 0.4 + rationalization_score * 0.2) * 100

    risk_category = "high" if total_score >= 70 else "medium" if total_score >= 40 else "low"

    high_risk_areas = []
    if opportunity_score > 0.3:
        high_risk_areas.append("Revenue recognition")
        high_risk_areas.append("Management override")
    if pressure_score > 0.3:
        high_risk_areas.append("Financial position manipulation")
    if rationalization_score > 0.2:
        high_risk_areas.append("Expense capitalization")

    return FraudRiskAssessmentResponse(
        audit_id=request.audit_id,
        fraud_risk_score=round(total_score, 2),
        risk_category=risk_category,
        pressure_indicators=pressure_indicators if pressure_indicators else [FraudIndicator(category="pressure", indicator="No significant pressures identified", risk_level="low", weight=0.0)],
        opportunity_indicators=opportunity_indicators if opportunity_indicators else [FraudIndicator(category="opportunity", indicator="Controls appear adequate", risk_level="low", weight=0.0)],
        rationalization_indicators=rationalization_indicators if rationalization_indicators else [FraudIndicator(category="rationalization", indicator="No concerning rationalization", risk_level="low", weight=0.0)],
        high_risk_areas=high_risk_areas if high_risk_areas else ["No high risk areas identified"],
        recommended_procedures=["Enhanced management inquiry", "Journal entry testing", "Significant estimates review", "Related party transaction testing"] if risk_category == "high" else ["Standard substantive procedures"],
        conclusion=f"Fraud risk assessed as {risk_category} with score of {round(total_score, 2)}"
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8199)
