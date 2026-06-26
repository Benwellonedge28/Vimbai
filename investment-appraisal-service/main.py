"""
Investment Appraisal Service
Port: 8233
Multi-criteria investment evaluation
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Investment Appraisal Service", version="1.0.0")

class AppraisalResult(BaseModel):
    project_id: str
    financial_score: float
    strategic_score: float
    risk_score: float
    environmental_score: float
    social_score: float
    governance_score: float
    overall_score: float
    recommendation: str

class InvestmentAppraisalRequest(BaseModel):
    company_id: str
    projects: List[Dict[str, Any]]
    evaluation_criteria: Dict[str, float]
    weights: Dict[str, float]

class InvestmentAppraisalResponse(BaseModel):
    company_id: str
    appraisal_results: List[AppraisalResult]
    best_project: str
    recommended_projects: List[str]
    recommendations: List[str]

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
    return {"status": "healthy", "service": "investment-appraisal", "version": "1.0.0"}

@app.post("/appraise", response_model=InvestmentAppraisalResponse)
async def appraise_investments(request: InvestmentAppraisalRequest):
    logger.info("Appraising investments", company=request.company_id)

    appraisal_results = []
    weights = request.weights or {"financial": 0.4, "strategic": 0.2, "risk": 0.2, "esg": 0.2}

    for project in request.projects:
        fin_score = project.get("npv", 0) / 1000000
        strat_score = project.get("strategic_value", 7) / 10
        risk_score = 1 - project.get("risk_level", 0.5)
        env_score = project.get("environmental_impact", 7) / 10
        soc_score = project.get("social_impact", 7) / 10
        gov_score = project.get("governance_score", 7) / 10

        overall = (
            fin_score * weights.get("financial", 0.4) +
            strat_score * weights.get("strategic", 0.2) +
            risk_score * weights.get("risk", 0.2) +
            (env_score + soc_score + gov_score) / 3 * weights.get("esg", 0.2)
        )

        appraisal_results.append(AppraisalResult(
            project_id=project.get("id", ""),
            financial_score=round(fin_score, 2),
            strategic_score=round(strat_score, 2),
            risk_score=round(risk_score, 2),
            environmental_score=round(env_score, 2),
            social_score=round(soc_score, 2),
            governance_score=round(gov_score, 2),
            overall_score=round(overall, 4),
            recommendation="approve" if overall > 0.6 else "review" if overall > 0.4 else "reject"
        ))

    ranked = sorted(appraisal_results, key=lambda x: x.overall_score, reverse=True)
    best = ranked[0].project_id if ranked else ""
    recommended = [r.project_id for r in ranked if r.recommendation == "approve"][:3]

    return InvestmentAppraisalResponse(
        company_id=request.company_id,
        appraisal_results=appraisal_results,
        best_project=best,
        recommended_projects=recommended,
        recommendations=["Prioritize highest scoring investments", "Review medium-scored projects for improvement", "Consider portfolio diversification"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8233)
