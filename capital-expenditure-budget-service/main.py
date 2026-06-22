"""
Capital Expenditure Budget Service
Port: 8178
CapEx planning, project ranking, payback period budgeting
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Capital Expenditure Budget Service", version="1.0.0")

class CapExProject(BaseModel):
    project_id: str
    project_name: str
    initial_investment: float
    annual_cash_flow: float
    project_life: int
    priority: int

class CapExBudgetRequest(BaseModel):
    company_id: str
    budget_year: str
    total_capex_budget: float
    projects: List[CapExProject]

class CapExBudgetResponse(BaseModel):
    company_id: str
    budget_year: str
    total_capex_budget: float
    funded_projects: List[Dict[str, Any]]
    deferred_projects: List[Dict[str, Any]]
    total_payback_period: float
    expected_roi: float

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
    return {"status": "healthy", "service": "capital-expenditure-budget", "version": "1.0.0"}

@app.post("/prepare", response_model=CapExBudgetResponse)
async def prepare_capex_budget(request: CapExBudgetRequest):
    logger.info("Preparing CapEx budget", company=request.company_id)

    sorted_projects = sorted(request.projects, key=lambda x: x.priority)
    remaining = request.total_capex_budget
    funded = []
    deferred = []
    total_roi = 0

    for project in sorted_projects:
        payback = project.initial_investment / project.annual_cash_flow if project.annual_cash_flow else 0
        total_return = project.annual_cash_flow * project.project_life
        roi = (total_return - project.initial_investment) / project.initial_investment * 100 if project.initial_investment else 0
        total_roi += roi

        if project.initial_investment <= remaining:
            funded.append({
                "project_id": project.project_id,
                "project_name": project.project_name,
                "initial_investment": project.initial_investment,
                "payback_period": round(payback, 1),
                "roi": round(roi, 2),
                "funded": True
            })
            remaining -= project.initial_investment
        else:
            deferred.append({
                "project_id": project.project_id,
                "project_name": project.project_name,
                "initial_investment": project.initial_investment,
                "deferred_reason": "Budget constraint"
            })

    return CapExBudgetResponse(
        company_id=request.company_id,
        budget_year=request.budget_year,
        total_capex_budget=request.total_capex_budget,
        funded_projects=funded,
        deferred_projects=deferred,
        total_payback_period=round(sum(p["payback_period"] for p in funded) / len(funded), 1) if funded else 0,
        expected_roi=round(total_roi / len(request.projects), 2) if request.projects else 0
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8178)
