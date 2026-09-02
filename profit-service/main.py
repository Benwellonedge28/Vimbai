"""
Vimbai Profit Service
Calculates profit for investment appraisal decisions.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "profit-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8101"))

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Profit Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class ProfitCalculation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    revenue: float
    total_costs: float
    profit: float = 0
    profit_margin: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Profit calculation service"}


@app.post("/calculate")
async def calculate_profit(revenue: float, total_costs: float):
    """Calculate profit from revenue and costs."""
    profit = revenue - total_costs
    profit_margin = (profit / revenue * 100) if revenue > 0 else 0

    calc = ProfitCalculation(revenue=revenue, total_costs=total_costs, profit=profit, profit_margin=profit_margin)
    return calc


@app.post("/from-cash-flow")
async def profit_from_cash_flow(cash_inflows: float, cash_outflows: float):
    """Calculate profit from cash flows."""
    net_cash = cash_inflows - cash_outflows
    return {
        "cash_inflows": cash_inflows,
        "cash_outflows": cash_outflows,
        "net_cash_flow": net_cash,
        "note": "This represents accounting profit (not cash profit)",
    }


@app.post("/average-profit")
async def calculate_average_profit(annual_profits: List[float]):
    """Calculate average annual profit."""
    if not annual_profits:
        return {"average_profit": 0}
    avg = sum(annual_profits) / len(annual_profits)
    return {"annual_profits": annual_profits, "number_of_years": len(annual_profits), "average_profit": avg}


@app.post("/average-accounting-profit")
async def average_accounting_profit(total_revenues: float, total_costs: float, years: int):
    """Calculate average accounting profit over years."""
    if years <= 0:
        return {"error": "Years must be positive"}

    annual_profit = (total_revenues - total_costs) / years
    return {
        "total_revenues": total_revenues,
        "total_costs": total_costs,
        "years": years,
        "average_annual_profit": annual_profit,
    }


@app.post("/net-profit")
async def net_profit_after_tax(revenue: float, costs: float, tax_rate: float):
    """Calculate net profit after tax."""
    gross_profit = revenue - costs
    tax = gross_profit * (tax_rate / 100) if tax_rate > 0 else 0
    net_profit = gross_profit - tax
    return {
        "revenue": revenue,
        "costs": costs,
        "gross_profit": gross_profit,
        "tax_rate": tax_rate,
        "tax_amount": tax,
        "net_profit_after_tax": net_profit,
    }


@app.post("/profit-compare")
async def compare_profits(
    project_a_revenue: float, project_a_costs: float, project_b_revenue: float, project_b_costs: float
):
    """Compare profits between two projects."""
    profit_a = project_a_revenue - project_a_costs
    profit_b = project_b_revenue - project_b_costs
    diff = profit_a - profit_b

    return {
        "project_a": {"revenue": project_a_revenue, "costs": project_a_costs, "profit": profit_a},
        "project_b": {"revenue": project_b_revenue, "costs": project_b_costs, "profit": profit_b},
        "difference": diff,
        "recommendation": "Project A" if profit_a > profit_b else "Project B" if profit_b > profit_a else "Equal",
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
