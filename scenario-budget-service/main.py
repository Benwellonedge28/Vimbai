"""
Scenario Budget Service
Port: 8175
Best case/worst case/most likely scenarios
"""

from typing import Any, Dict

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Scenario Budget Service", version="1.0.0")


class ScenarioBudgetRequest(BaseModel):
    company_id: str
    base_revenue: float
    base_expenses: float
    worst_case_multiplier: float
    best_case_multiplier: float


class ScenarioResult(BaseModel):
    scenario: str
    revenue: float
    expenses: float
    profit: float
    profit_margin: float


class ScenarioBudgetResponse(BaseModel):
    company_id: str
    worst_case: ScenarioResult
    most_likely: ScenarioResult
    best_case: ScenarioResult
    risk_assessment: str


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
    return {"status": "healthy", "service": "scenario-budget", "version": "1.0.0"}


@app.post("/analyze", response_model=ScenarioBudgetResponse)
async def analyze_scenarios(request: ScenarioBudgetRequest):
    logger.info("Analyzing budget scenarios", company=request.company_id)

    worst_rev = request.base_revenue * request.worst_case_multiplier
    worst_exp = request.base_expenses * 0.9
    worst_profit = worst_rev - worst_exp

    most_rev = request.base_revenue
    most_exp = request.base_expenses
    most_profit = most_rev - most_exp

    best_rev = request.base_revenue * request.best_case_multiplier
    best_exp = request.base_expenses * 1.1
    best_profit = best_rev - best_exp

    return ScenarioBudgetResponse(
        company_id=request.company_id,
        worst_case=ScenarioResult(
            scenario="Worst Case",
            revenue=round(worst_rev, 2),
            expenses=round(worst_exp, 2),
            profit=round(worst_profit, 2),
            profit_margin=round(worst_profit / worst_rev * 100, 2) if worst_rev else 0,
        ),
        most_likely=ScenarioResult(
            scenario="Most Likely",
            revenue=round(most_rev, 2),
            expenses=round(most_exp, 2),
            profit=round(most_profit, 2),
            profit_margin=round(most_profit / most_rev * 100, 2) if most_rev else 0,
        ),
        best_case=ScenarioResult(
            scenario="Best Case",
            revenue=round(best_rev, 2),
            expenses=round(best_exp, 2),
            profit=round(best_profit, 2),
            profit_margin=round(best_profit / best_rev * 100, 2) if best_rev else 0,
        ),
        risk_assessment=(
            "Medium risk - wide range between scenarios"
            if abs(worst_profit - best_profit) > request.base_profit
            else "Low risk"
        ),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8175)
