"""
Going Concern Service
Port: 8203
Going concern assessment under IAS/IFRS
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Going Concern Service", version="1.0.0")

class FinancialStressIndicators(BaseModel):
    negative_working_capital: bool
    borrowing_agreement_breaches: bool
    covenant_violations: bool
    going_concern_modifications: bool
    arrears_on_dividends: bool
    loan_repayment_extensions: bool

class GoingConcernRequest(BaseModel):
    company_id: str
    audit_id: str
    financial_data: Dict[str, float]
    debt_covenants: List[Dict[str, Any]]
    cash_flows: Dict[str, float]
    management_plans: List[str]
    financing_arrangements: List[str]

class GoingConcernResponse(BaseModel):
    company_id: str
    audit_id: str
    assessment_date: str
    risk_indicators: FinancialStressIndicators
    liquidity_score: float
    solvency_score: float
    cash_flow_adequacy: float
    assessment_result: str
    material_uncertainty: bool
    mitigating_factors: List[str]
    conclusions: Dict[str, str]

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
    return {"status": "healthy", "service": "going-concern", "version": "1.0.0"}

@app.post("/assess", response_model=GoingConcernResponse)
async def assess_going_concern(request: GoingConcernRequest):
    logger.info("Assessing going concern", company=request.company_id, audit=request.audit_id)

    financial = request.financial_data
    debt = sum(d.get("amount", 0) for d in request.debt_covenants)
    assets = financial.get("current_assets", 0) + financial.get("non_current_assets", 0)
    liabilities = financial.get("current_liabilities", 0) + financial.get("non_current_liabilities", 0)

    working_capital = financial.get("current_assets", 0) - financial.get("current_liabilities", 0)
    current_ratio = financial.get("current_assets", 0) / financial.get("current_liabilities", 1)
    debt_ratio = liabilities / assets if assets else 1

    liquidity_score = min(100, max(0, (current_ratio - 0.5) * 100))
    solvency_score = min(100, max(0, (1 - debt_ratio) * 100))

    operating_cf = request.cash_flows.get("operating", 0)
    financing_cf = request.cash_flows.get("financing", 0)
    cash_flow_adequacy = (operating_cf + financing_cf) / (debt + 1) if debt else 0.5

    indicators = FinancialStressIndicators(
        negative_working_capital=working_capital < 0,
        borrowing_agreement_breaches=any(d.get("breach", False) for d in request.debt_covenants),
        covenant_violations=any(d.get("violation", False) for d in request.debt_covenants),
        going_concern_modifications=financial.get("going_concern_modifications", 0) > 0,
        arrears_on_dividends=financial.get("dividend_arrears", 0) > 0,
        loan_repayment_extensions=financial.get("extensions_granted", 0) > 0
    )

    risk_count = sum([
        indicators.negative_working_capital,
        indicators.borrowing_agreement_breaches,
        indicators.covenant_violations,
        indicators.going_concern_modifications
    ])

    if risk_count >= 3 or liquidity_score < 30:
        result = "high_risk"
        material_uncertainty = True
    elif risk_count >= 1 or liquidity_score < 50:
        result = "medium_risk"
        material_uncertainty = False
    else:
        result = "low_risk"
        material_uncertainty = False

    mitigating = []
    if request.management_plans:
        mitigating.append("Management has prepared mitigation plans")
    if request.financing_arrangements:
        mitigating.append("Committed financing facilities in place")
    if cash_flow_adequacy > 0:
        mitigating.append("Positive cash flow from operations")

    return GoingConcernResponse(
        company_id=request.company_id,
        audit_id=request.audit_id,
        assessment_date="2024-06-30",
        risk_indicators=indicators,
        liquidity_score=round(liquidity_score, 2),
        solvency_score=round(solvency_score, 2),
        cash_flow_adequacy=round(cash_flow_adequacy, 4),
        assessment_result=result,
        material_uncertainty=material_uncertainty,
        mitigating_factors=mitigating if mitigating else ["No specific mitigating factors identified"],
        conclusions={
            "going_concern": "Basis for concern exists" if result == "high_risk" else "No significant doubts",
            "disclosure": "Material uncertainty disclosure required" if material_uncertainty else "Standard disclosure",
            "opinion_impact": "Qualified or emphasis of matter" if material_uncertainty else "Unqualified"
        }
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8203)
