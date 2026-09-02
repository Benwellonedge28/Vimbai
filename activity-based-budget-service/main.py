"""
Activity-Based Budget Service
Port: 8177
Budgeting based on activity cost pools and cost drivers
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Activity-Based Budget Service", version="1.0.0")


class ActivityPool(BaseModel):
    activity_id: str
    activity_name: str
    cost_pool: float
    cost_driver: str
    driver_volume: int
    rate_per_driver: float


class ActivityBudgetRequest(BaseModel):
    company_id: str
    budget_year: str
    activities: List[ActivityPool]


class ActivityBudgetResponse(BaseModel):
    company_id: str
    budget_year: str
    activity_budgets: List[Dict[str, Any]]
    total_cost_pool: float
    cost_per_unit: float


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
    return {"status": "healthy", "service": "activity-based-budget", "version": "1.0.0"}


@app.post("/prepare", response_model=ActivityBudgetResponse)
async def prepare_activity_budget(request: ActivityBudgetRequest):
    logger.info("Preparing activity-based budget", company=request.company_id)

    activity_budgets = []
    total_cost = 0

    for activity in request.activities:
        rate = activity.cost_pool / activity.driver_volume if activity.driver_volume else 0
        total_cost += activity.cost_pool

        activity_budgets.append(
            {
                "activity_id": activity.activity_id,
                "activity_name": activity.activity_name,
                "cost_pool": activity.cost_pool,
                "driver_volume": activity.driver_volume,
                "rate_per_driver": round(rate, 2),
                "cost_driver": activity.cost_driver,
            }
        )

    return ActivityBudgetResponse(
        company_id=request.company_id,
        budget_year=request.budget_year,
        activity_budgets=activity_budgets,
        total_cost_pool=total_cost,
        cost_per_unit=(
            round(total_cost / sum(a.driver_volume for a in request.activities), 2) if request.activities else 0
        ),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8177)
