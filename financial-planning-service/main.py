"""
Financial Planning Service
Port: 8231
Comprehensive financial planning and forecasting
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Financial Planning Service", version="1.0.0")


class FinancialPlan(BaseModel):
    company_id: str
    planning_period: str
    revenue_projections: List[float]
    cost_projections: List[float]
    capital_expenditure_plan: List[float]
    working_capital_requirements: List[float]
    financing_plan: Dict[str, Any]


class FinancialPlanningRequest(BaseModel):
    company_id: str
    fiscal_year: str
    historical_data: Dict[str, Any]
    growth_assumptions: Dict[str, float]
    strategic_initiatives: List[Dict[str, Any]]


class FinancialPlanningResponse(BaseModel):
    company_id: str
    fiscal_year: str
    financial_plan: FinancialPlan
    key_assumptions: Dict[str, float]
    sensitivity_analysis: Dict[str, List[float]]
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
    return {"status": "healthy", "service": "financial-planning", "version": "1.0.0"}


@app.post("/plan", response_model=FinancialPlanningResponse)
async def create_financial_plan(request: FinancialPlanningRequest):
    logger.info("Creating financial plan", company=request.company_id, year=request.fiscal_year)

    base_revenue = request.historical_data.get("revenue", 10000000)
    growth_rate = request.growth_assumptions.get("revenue_growth", 0.1)

    revenue_projections = [base_revenue * (1 + growth_rate) ** i for i in range(1, 6)]
    cost_projections = [r * 0.7 for r in revenue_projections]
    capex = [r * 0.1 for r in revenue_projections]
    working_capital = [r * 0.15 for r in revenue_projections]

    financial_plan = FinancialPlan(
        company_id=request.company_id,
        planning_period=request.fiscal_year,
        revenue_projections=[round(r, 2) for r in revenue_projections],
        cost_projections=[round(c, 2) for c in cost_projections],
        capital_expenditure_plan=[round(c, 2) for c in capex],
        working_capital_requirements=[round(w, 2) for w in working_capital],
        financing_plan={"debt_ratio": 0.4, "equity_ratio": 0.6},
    )

    sensitivity = {
        "revenue_growth": [r * 0.9 for r in revenue_projections],
        "cost_increase": [c * 1.05 for c in cost_projections],
    }

    return FinancialPlanningResponse(
        company_id=request.company_id,
        fiscal_year=request.fiscal_year,
        financial_plan=financial_plan,
        key_assumptions=request.growth_assumptions,
        sensitivity_analysis=sensitivity,
        recommendations=[
            "Monitor revenue growth assumptions quarterly",
            "Review cost structure annually",
            "Maintain capital reserve for contingencies",
        ],
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8231)
