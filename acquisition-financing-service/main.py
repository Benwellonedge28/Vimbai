"""
Acquisition Financing Service
Port: 8245
M&A financing structure optimization
"""

import math
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Acquisition Financing Service", version="1.0.0")


class FinancingTranche(BaseModel):
    type: str
    amount: float
    rate: float
    term_years: int
    amortization: str


class AcquisitionFinancingRequest(BaseModel):
    deal_id: str
    acquirer_id: str
    target_id: str
    deal_value: float
    acquirer_equity: float
    acquirer_debt_capacity: float
    current_leverage: float
    target_leverage: float
    interest_rate: float
    equity_cost: float


class AcquisitionFinancingResponse(BaseModel):
    deal_id: str
    optimal_structure: Dict[str, Any]
    financing_options: List[Dict[str, Any]]
    leverage_analysis: Dict[str, Any]
    cost_of_capital: float
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "acquisition-financing", "version": "1.0.0"}


@app.post("/structure", response_model=AcquisitionFinancingResponse)
async def structure_acquisition_financing(request: AcquisitionFinancingRequest):
    logger.info("Structuring acquisition financing", deal=request.deal_id)

    debt_capacity = min(request.acquirer_debt_capacity, request.target_leverage * request.deal_value)
    equity_required = request.deal_value - debt_capacity
    equity_percentage = equity_required / request.deal_value

    leverage = debt_capacity / request.deal_value
    interest_expense = debt_capacity * request.interest_rate
    after_tax_cost = interest_expense * (1 - 0.25)

    wacc = leverage * request.interest_rate * 0.75 + (1 - leverage) * request.equity_cost

    senior_debt = debt_capacity * 0.6
    mezzanine = debt_capacity * 0.25
    subordinated = debt_capacity * 0.15

    senior_rate = request.interest_rate
    mezzanine_rate = request.interest_rate + 0.03
    sub_rate = request.interest_rate + 0.06

    tranches = [
        {"type": "Senior Debt", "amount": senior_debt, "rate": senior_rate, "term": 5, "percentage": 60},
        {"type": "Mezzanine", "amount": mezzanine, "rate": mezzanine_rate, "term": 7, "percentage": 25},
        {"type": "Equity", "amount": equity_required, "rate": request.equity_cost, "term": 0, "percentage": 15},
    ]

    option_low_leverage = {
        "debt_percentage": 40,
        "debt": request.deal_value * 0.4,
        "equity": request.deal_value * 0.6,
        "wacc": 0.4 * request.interest_rate * 0.75 + 0.6 * request.equity_cost,
        "interest_coverage": (
            request.deal_value * 0.2 / (request.deal_value * 0.4 * request.interest_rate)
            if request.interest_rate
            else 0
        ),
    }

    option_high_leverage = {
        "debt_percentage": 70,
        "debt": request.deal_value * 0.7,
        "equity": request.deal_value * 0.3,
        "wacc": 0.7 * request.interest_rate * 0.75 + 0.3 * request.equity_cost,
        "interest_coverage": (
            request.deal_value * 0.2 / (request.deal_value * 0.7 * request.interest_rate)
            if request.interest_rate
            else 0
        ),
    }

    optimal = {
        "debt": debt_capacity,
        "equity": equity_required,
        "debt_percentage": round(leverage * 100, 2),
        "equity_percentage": round(equity_percentage * 100, 2),
        "wacc": round(wacc * 100, 2),
    }

    recommendations = []
    if equity_percentage > 0.6:
        recommendations.append("High equity contribution - consider taking more debt to optimize returns")
    if wacc > request.equity_cost:
        recommendations.append("Debt financing reduces WACC - consider higher leverage")
    if interest_expense > request.deal_value * 0.15:
        recommendations.append("Interest burden is significant - model stress scenarios")

    return AcquisitionFinancingResponse(
        deal_id=request.deal_id,
        optimal_structure={"tranches": tranches, "total_debt": debt_capacity, "total_equity": equity_required},
        financing_options=[option_low_leverage, optimal, option_high_leverage],
        leverage_analysis={
            "total_leverage": round(leverage, 4),
            "interest_expense": interest_expense,
            "after_tax_cost": after_tax_cost,
        },
        cost_of_capital=round(wacc * 100, 2),
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8245)
