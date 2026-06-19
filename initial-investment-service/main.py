"""
FinAcc Initial Investment Service
Handles initial investment calculations for capital projects.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "initial-investment-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8106"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Initial Investment Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class InvestmentDetail(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    item: str
    amount: float
    category: str  # asset, working_capital, installation, other


class InitialInvestment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    project_name: str
    items: List[InvestmentDetail]
    total_investment: float = 0
    scrap_value: float = 0
    net_investment: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


investments: List[InitialInvestment] = []


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
    scrap_value: float = 0
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
        project_name=project_name, items=items,
        total_investment=total, scrap_value=scrap_value, net_investment=net_investment
    )
    investments.append(inv)
    return inv


@app.post("/simple")
async def simple_initial_investment(
    asset_cost: float,
    scrap_value: float = 0,
    selling_of_old_asset: float = 0,
    tax_on_sale: float = 0
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
        "initial_cash_outlay": initial_outlay
    }


@app.post("/with-capital-allowance")
async def investment_with_capital_allowance(
    asset_cost: float,
    scrap_value: float,
    capital_allowance: float,
    tax_rate: float
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
        "adjusted_investment_cost": adjusted_cost
    }


@app.get("/list")
async def list_investments():
    """List all initial investments."""
    return {"investments": investments}


@app.get("/get/{investment_id}")
async def get_investment(investment_id: str):
    """Get specific investment by ID."""
    for inv in investments:
        if inv.id == investment_id:
            return inv
    return {"error": "Investment not found"}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
