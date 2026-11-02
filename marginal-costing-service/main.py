"""
Vimbai Marginal Costing Service
Manages variable costing / marginal costing methods.
"""

import importlib.util
import os
import os as _os
import sys as _sys
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "marginal_costing_service" not in _sys.modules or not hasattr(
    _sys.modules.get("marginal_costing_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("marginal_costing_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["marginal_costing_service"] = _pkg
    _sys.modules["marginal_costing_service"].__path__ = [_HERE]

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from marginal_costing_service import cost_calc, crud
from marginal_costing_service.dependencies import book_id_var, get_db_session, get_user_id
from marginal_costing_service.models import (
    ContributionAnalysis,
    MarginalCost,
    MarginalIncomeStatement,
)
from neo4j import AsyncSession

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Marginal costing calculations"}


@app.post("/costs/add")
async def add_marginal_cost(
    cost_name: str,
    cost_code: str,
    variable_cost: float,
    fixed_cost: float = 0,
    units: float = 1,
    cost_driver: str = "units",
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
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
    return await crud.create(session, user_id, cost)


@app.post("/income-statement/generate")
async def generate_marginal_income_statement(
    company_id: str,
    period: str,
    sales_revenue: float,
    variable_costs: Dict[str, float],
    fixed_costs: float,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
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

    return await crud.create(session, user_id, statement)


@app.post("/contribution/analyze")
async def analyze_contribution(
    product_id: str,
    selling_price: float,
    variable_cost_per_unit: float,
    units_sold: float,
    fixed_costs: float = 0,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
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

    return await crud.create(session, user_id, analysis)


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
async def list_income_statements(
    company_id: Optional[str] = None,
    period: Optional[str] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List marginal income statements."""
    result = await crud.list_all(session, user_id, MarginalIncomeStatement)
    if company_id:
        result = [s for s in result if s.company_id == company_id]
    if period:
        result = [s for s in result if s.period == period]
    return {"statements": result}


@app.get("/contributions")
async def list_contribution_analyses(
    product_id: Optional[str] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List contribution analyses."""
    result = await crud.list_all(session, user_id, ContributionAnalysis)
    if product_id:
        result = [c for c in result if c.product_id == product_id]
    return {"contributions": result}


@app.get("/summary")
async def get_marginal_cost_summary(
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Get marginal cost summary."""
    costs = await crud.list_all(session, user_id, MarginalCost)
    total_variable = sum(c.variable_cost for c in costs)
    total_fixed = sum(c.fixed_cost for c in costs)

    return {
        "total_variable_costs": total_variable,
        "total_fixed_costs": total_fixed,
        "total_costs": total_variable + total_fixed,
        "cost_items": len(costs),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
