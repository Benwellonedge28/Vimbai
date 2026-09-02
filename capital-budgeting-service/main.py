"""
Capital Budgeting Service
Port: 8232
Capital investment appraisal and ranking
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Capital Budgeting Service", version="1.0.0")


class InvestmentProject(BaseModel):
    project_id: str
    initial_investment: float
    cash_flows: List[float]
    npv: float
    irr: float
    payback_period: float
    profitability_index: float
    accept: bool


class CapitalBudgetingRequest(BaseModel):
    company_id: str
    planning_period: str
    projects: List[Dict[str, Any]]
    cost_of_capital: float
    capital_budget_limit: float


class CapitalBudgetingResponse(BaseModel):
    company_id: str
    planning_period: str
    projects_analyzed: List[InvestmentProject]
    optimal_portfolio: List[str]
    total_investment: float
    average_irr: float
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
    return {"status": "healthy", "service": "capital-budgeting", "version": "1.0.0"}


@app.post("/analyze", response_model=CapitalBudgetingResponse)
async def analyze_capital_budget(request: CapitalBudgetingRequest):
    logger.info("Analyzing capital budget", company=request.company_id)

    projects_analyzed = []
    total_investment = 0.0
    irr_values = []

    for project in request.projects:
        investment = project.get("initial_investment", 0)
        cash_flows = project.get("cash_flows", [])
        total_investment += investment

        pv = sum(cf / ((1 + request.cost_of_capital) ** (i + 1)) for i, cf in enumerate(cash_flows))
        npv = pv - investment
        pi = pv / investment if investment else 0

        cumulative = 0
        payback = len(cash_flows)
        for i, cf in enumerate(cash_flows):
            cumulative += cf
            if cumulative >= investment:
                payback = i + 1
                break

        irr = request.cost_of_capital + (npv / investment * 0.1) if investment else 0

        projects_analyzed.append(
            InvestmentProject(
                project_id=project.get("id", ""),
                initial_investment=investment,
                cash_flows=cash_flows,
                npv=round(npv, 2),
                irr=round(irr, 4),
                payback_period=payback,
                profitability_index=round(pi, 4),
                accept=npv > 0 and pi > 1,
            )
        )
        irr_values.append(irr)

    ranked = sorted(projects_analyzed, key=lambda x: x.npv, reverse=True)
    optimal = [p.project_id for p in ranked if p.accept][:5]

    return CapitalBudgetingResponse(
        company_id=request.company_id,
        planning_period=request.planning_period,
        projects_analyzed=projects_analyzed,
        optimal_portfolio=optimal,
        total_investment=round(total_investment, 2),
        average_irr=round(sum(irr_values) / len(irr_values), 4) if irr_values else 0,
        recommendations=[
            "Focus on high NPV projects within budget constraints",
            "Consider project interdependencies",
            "Review IRR against cost of capital",
        ],
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8232)
