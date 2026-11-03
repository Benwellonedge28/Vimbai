"""
Fraud Risk Assessment Service
Port: 8199
Fraud triangle analysis, risk indicators
"""

import importlib.util
import os
import sys
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("fraud_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["fraud_calc"] = calc
_spec.loader.exec_module(calc)

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
    return {
        "status": "healthy",
        "service": "fraud-risk-assessment",
        "version": "1.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/assess", response_model=FraudRiskAssessmentResponse)
async def assess_fraud_risk(request: FraudRiskAssessmentRequest):
    logger.info("Assessing fraud risk", audit=request.audit_id, company=request.company_id)
    result = calc.assess(
        request.audit_id,
        request.company_id,
        request.industry,
        request.company_size,
        request.financial_indicators,
        request.management_characteristics,
        request.internal_control_gaps,
        request.prior_fraud_history,
    )
    return FraudRiskAssessmentResponse(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8199)
