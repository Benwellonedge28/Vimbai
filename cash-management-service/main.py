"""
Cash Management Service
Port: 8264
Cash management optimization
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Cash Management Service", version="1.0.0")


class CashManagementRequest(BaseModel):
    company_id: str
    total_cash: float
    monthly_burn_rate: float
    target_reserve_months: int
    investment_options: Dict[str, float]


class CashManagementResponse(BaseModel):
    company_id: str
    cash_position: Dict[str, Any]
    allocation_plan: Dict[str, Any]
    runway_analysis: Dict[str, Any]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "cash-management", "version": "1.0.0"}


@app.post("/analyze", response_model=CashManagementResponse)
async def analyze_cash_management(request: CashManagementRequest):
    logger.info("Analyzing cash management", company=request.company_id)

    target_reserve = request.monthly_burn_rate * request.target_reserve_months
    investable_cash = max(0, request.total_cash - target_reserve)

    cash_position = {
        "total_cash": request.total_cash,
        "target_reserve": round(target_reserve, 2),
        "investable_cash": round(investable_cash, 2),
        "monthly_burn": request.monthly_burn_rate,
    }

    allocation = {}
    remaining = investable_cash
    for option, max_allocation in request.investment_options.items():
        allocation[option] = min(remaining, max_allocation)
        remaining -= allocation[option]

    allocation_plan = {
        "allocations": {k: round(v, 2) for k, v in allocation.items()},
        "unallocated": round(remaining, 2),
        "expected_return": round(sum(a * 0.04 for a in allocation.values()), 2),
    }

    runway_months = request.total_cash / request.monthly_burn_rate if request.monthly_burn_rate else 0

    runway_analysis = {
        "runway_months": round(runway_months, 2),
        "target_months": request.target_reserve_months,
        "status": (
            "Healthy"
            if runway_months >= request.target_reserve_months
            else "Warning" if runway_months >= 6 else "Critical"
        ),
    }

    recommendations = []
    if runway_months < request.target_reserve_months:
        recommendations.append("Runway below target - consider raising capital")
    if investable_cash > 0:
        recommendations.append("Deploy excess cash to investment accounts for yield")

    return CashManagementResponse(
        company_id=request.company_id,
        cash_position=cash_position,
        allocation_plan=allocation_plan,
        runway_analysis=runway_analysis,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8264)
