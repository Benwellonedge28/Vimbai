"""
Profitability Index Service
Port: 8217
PI and investment decision analysis
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Profitability Index Service", version="1.0.0")


class ProjectAnalysis(BaseModel):
    project_id: str
    initial_investment: float
    pv_of_future_cash_flows: float
    profitability_index: float
    npv: float
    accept: bool


class PIRequest(BaseModel):
    company_id: str
    discount_rate: float
    projects: List[Dict[str, Any]]


class PIResponse(BaseModel):
    company_id: str
    discount_rate: float
    project_analysis: List[ProjectAnalysis]
    total_investment_required: float
    ranking_by_pi: List[Dict[str, Any]]
    optimal_portfolio: List[str]
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
    return {"status": "healthy", "service": "profitability-index", "version": "1.0.0"}


@app.post("/analyze", response_model=PIResponse)
async def analyze_profitability_index(request: PIRequest):
    logger.info("Analyzing profitability index", company=request.company_id)

    project_analysis = []
    total_investment = 0.0

    for project in request.projects:
        investment = project.get("initial_investment", 0)
        cash_flows = project.get("cash_flows", [])
        total_investment += investment

        pv = 0
        for i, cf in enumerate(cash_flows):
            pv += cf / ((1 + request.discount_rate) ** (i + 1))

        pi = pv / investment if investment else 0
        npv = pv - investment

        project_analysis.append(
            ProjectAnalysis(
                project_id=project.get("id", ""),
                initial_investment=investment,
                pv_of_future_cash_flows=round(pv, 2),
                profitability_index=round(pi, 4),
                npv=round(npv, 2),
                accept=pi > 1.0,
            )
        )

    ranked = sorted(project_analysis, key=lambda x: x.profitability_index, reverse=True)
    ranking = [{"rank": i + 1, "project": p.project_id, "pi": p.profitability_index} for i, p in enumerate(ranked)]

    optimal = [p.project_id for p in ranked if p.accept]

    return PIResponse(
        company_id=request.company_id,
        discount_rate=request.discount_rate,
        project_analysis=project_analysis,
        total_investment_required=round(total_investment, 2),
        ranking_by_pi=ranking,
        optimal_portfolio=optimal if optimal else ["No projects meet acceptance criteria"],
        recommendations=[
            "Accept all projects with PI > 1.0",
            "Prioritize projects by PI ranking",
            "Consider risk-adjusted returns for projects with similar PI",
        ],
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8217)
