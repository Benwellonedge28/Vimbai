"""
Vimbai Initial Investment Service
Handles initial investment calculations for capital projects.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "initial_investment_service" not in _sys.modules or not hasattr(
    _sys.modules.get("initial_investment_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("initial_investment_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["initial_investment_service"] = _pkg
    _sys.modules["initial_investment_service"].__path__ = [_HERE]

import os
from typing import Any, Dict, List

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from initial_investment_service import crud
from initial_investment_service.dependencies import book_id_var, get_db_session, get_user_id
from initial_investment_service.models import InitialInvestment, InvestmentDetail
from neo4j import AsyncSession

SERVICE_NAME = "initial-investment-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8106"))

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

app = FastAPI(title="Vimbai Initial Investment Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Initial investment calculations"}


@app.post("/create")
async def create_initial_investment(
    project_name: str,
    asset_cost: float,
    installation_cost: float = 0,
    working_capital: float = 0,
    other_costs: float = 0,
    scrap_value: float = 0,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Create initial investment calculation."""
    items = [
        InvestmentDetail(item="Asset Cost", amount=asset_cost, category="asset"),
    ]
    if installation_cost > 0:
        items.append(InvestmentDetail(item="Installation Cost", amount=installation_cost, category="installation"))
    if working_capital > 0:
        items.append(InvestmentDetail(item="Working Capital", amount=working_capital, category="working_capital"))
    if other_costs > 0:
        items.append(InvestmentDetail(item="Other Costs", amount=other_costs, category="other"))

    total = asset_cost + installation_cost + working_capital + other_costs
    net_investment = total - scrap_value

    inv = InitialInvestment(
        project_name=project_name,
        items=items,
        total_investment=total,
        scrap_value=scrap_value,
        net_investment=net_investment,
    )
    return await crud.create(session, user_id, inv)


@app.post("/simple")
async def simple_initial_investment(
    asset_cost: float, scrap_value: float = 0, selling_of_old_asset: float = 0, tax_on_sale: float = 0
):
    """Simple initial investment calculation."""
    net_proceeds = selling_of_old_asset - tax_on_sale
    initial_outlay = asset_cost - net_proceeds

    return {
        "asset_cost": asset_cost,
        "scrap_value": scrap_value,
        "selling_of_old_asset": selling_of_old_asset,
        "tax_on_sale": tax_on_sale,
        "net_proceeds_from_sale": net_proceeds,
        "initial_cash_outlay": initial_outlay,
    }


@app.post("/with-capital-allowance")
async def investment_with_capital_allowance(
    asset_cost: float, scrap_value: float, capital_allowance: float, tax_rate: float
):
    """Calculate investment with capital allowances."""
    tax_saving = capital_allowance * (tax_rate / 100)
    adjusted_cost = asset_cost - tax_saving

    return {
        "asset_cost": asset_cost,
        "scrap_value": scrap_value,
        "capital_allowance": capital_allowance,
        "tax_rate": tax_rate,
        "tax_saving": tax_saving,
        "adjusted_investment_cost": adjusted_cost,
    }


@app.get("/list")
async def list_investments(
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List all initial investments."""
    return {"investments": await crud.list_all(session, user_id, InitialInvestment)}


@app.get("/get/{investment_id}")
async def get_investment(
    investment_id: str,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Get specific investment by ID.

    The original answered 200 with an {"error": ...} body; that contract is kept.
    """
    inv = await crud.find(session, user_id, InitialInvestment, investment_id)
    if inv is None:
        return {"error": "Investment not found"}
    return inv


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
