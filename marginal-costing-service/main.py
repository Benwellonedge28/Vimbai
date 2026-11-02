"""
Vimbai Marginal Costing Service
Manages variable costing / marginal costing methods.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

from shared import cost_calc

SERVICE_NAME = "marginal-costing-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8066"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

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

app = FastAPI(title="Vimbai Marginal Costing Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class MarginalCost(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_name: str
    cost_code: str
    variable_cost: float  # Marginal/Variable cost
    fixed_cost: float = 0  # Period cost
    total_cost: float = 0
    unit_variable_cost: float = 0
    units: float = 1
    cost_driver: str
    created_at: datetime = Field(default_factory=datetime.utcnow)


class MarginalIncomeStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period: str
    sales_revenue: float
    total_variable_costs: float
    contribution: float = 0
    fixed_costs: float
    profit: float = 0
    variable_cost_breakdown: Dict[str, float] = {}
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ContributionAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    selling_price: float
    variable_cost_per_unit: float
    contribution_per_unit: float = 0
    contribution_margin_ratio: float = 0
    total_contribution: float = 0
    fixed_costs_allocated: float = 0
    profit_from_product: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


marginal_costs: List[MarginalCost] = []
income_statements: List[MarginalIncomeStatement] = []
contribution_analyses: List[ContributionAnalysis] = []


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "POST":
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception:
        return {}


@app.get("/health")
async def health_check():
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "status": "healthy",
        "calculation_engine": cost_calc.engine_in_use(),
    }


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Marginal costing management"}


@app.post("/costs/add")
async def add_marginal_cost(
    cost_name: str,
    cost_code: str,
    variable_cost: float,
    fixed_cost: float = 0,
    units: float = 1,
    cost_driver: str = "units",
):
    """Add a marginal cost item."""
    cost = MarginalCost(
        cost_name=cost_name,
        cost_code=cost_code,
        variable_cost=variable_cost,
        fixed_cost=fixed_cost,
        units=units,
        cost_driver=cost_driver,
    )
    figures = cost_calc.marginal_cost_item(cost_name, cost_code, variable_cost, fixed_cost, units, cost_driver)
    cost.total_cost = figures["total_cost"]
    cost.unit_variable_cost = figures["unit_variable_cost"]
    marginal_costs.append(cost)
    return cost


@app.post("/income-statement/generate")
async def generate_marginal_income_statement(
    company_id: str, period: str, sales_revenue: float, variable_costs: Dict[str, float], fixed_costs: float
):
    """Generate marginal income statement."""
    figures = cost_calc.marginal_income_statement(company_id, period, sales_revenue, variable_costs, fixed_costs)
    total_variable_costs = figures["total_variable_costs"]

    statement = MarginalIncomeStatement(
        company_id=company_id,
        period=period,
        sales_revenue=sales_revenue,
        total_variable_costs=total_variable_costs,
        fixed_costs=fixed_costs,
        variable_cost_breakdown=variable_costs,
    )

    statement.contribution = figures["contribution"]
    statement.profit = figures["profit"]

    income_statements.append(statement)
    return statement


@app.post("/contribution/analyze")
async def analyze_contribution(
    product_id: str, selling_price: float, variable_cost_per_unit: float, units_sold: float, fixed_costs: float = 0
):
    """Analyze contribution for a product."""
    analysis = ContributionAnalysis(
        product_id=product_id,
        selling_price=selling_price,
        variable_cost_per_unit=variable_cost_per_unit,
        fixed_costs_allocated=fixed_costs,
    )

    figures = cost_calc.marginal_contribution(
        product_id, selling_price, variable_cost_per_unit, units_sold, fixed_costs
    )
    analysis.contribution_per_unit = figures["contribution_per_unit"]
    analysis.contribution_margin_ratio = figures["contribution_margin_ratio"]
    analysis.total_contribution = figures["total_contribution"]
    analysis.profit_from_product = figures["profit_from_product"]

    contribution_analyses.append(analysis)
    return analysis


@app.post("/profit-forecast")
async def forecast_profit(
    selling_price: float, variable_cost_per_unit: float, fixed_costs: float, expected_units: float
):
    """Forecast profit using marginal costing."""
    figures = cost_calc.marginal_forecast(selling_price, variable_cost_per_unit, fixed_costs, expected_units)

    return {
        "selling_price": selling_price,
        "variable_cost_per_unit": variable_cost_per_unit,
        "contribution_per_unit": figures["contribution_per_unit"],
        "contribution_margin_ratio": figures["contribution_margin_ratio"],
        "expected_units": expected_units,
        "total_contribution": figures["total_contribution"],
        "fixed_costs": fixed_costs,
        "forecast_profit": figures["forecast_profit"],
    }


@app.get("/income-statements")
async def list_income_statements(company_id: Optional[str] = None, period: Optional[str] = None):
    """List marginal income statements."""
    result = income_statements
    if company_id:
        result = [s for s in result if s.company_id == company_id]
    if period:
        result = [s for s in result if s.period == period]
    return {"statements": result}


@app.get("/contributions")
async def list_contribution_analyses(product_id: Optional[str] = None):
    """List contribution analyses."""
    result = contribution_analyses
    if product_id:
        result = [c for c in result if c.product_id == product_id]
    return {"contributions": result}


@app.get("/summary")
async def get_marginal_cost_summary():
    """Get marginal cost summary."""
    total_variable = sum(c.variable_cost for c in marginal_costs)
    total_fixed = sum(c.fixed_cost for c in marginal_costs)

    return {
        "total_variable_costs": total_variable,
        "total_fixed_costs": total_fixed,
        "total_costs": total_variable + total_fixed,
        "cost_items": len(marginal_costs),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
