"""
Vimbai Investment Monitoring Service
Tracks investment portfolios, performance metrics, and market valuations.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "investment-monitoring-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8290"))

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

app = FastAPI(title="Vimbai Investment Monitoring Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class InvestmentHolding(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    portfolio_id: str
    instrument_name: str
    instrument_type: str  # equity, bond, etf, commodity, cash
    quantity: float
    purchase_price: float
    current_price: float
    currency: str = "USD"
    sector: str = ""
    updated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class Portfolio(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    target_return: float = 0.0
    risk_tolerance: str = "moderate"  # conservative, moderate, aggressive
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class PerformanceMetric(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    portfolio_id: str
    metric_date: datetime
    total_value: float
    total_cost: float
    unrealized_gain: float
    unrealized_gain_pct: float
    daily_return: float = 0.0
    cumulative_return: float = 0.0


portfolios: List[Portfolio] = []
holdings: List[InvestmentHolding] = []
performance: List[PerformanceMetric] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/portfolios", response_model=Portfolio)
async def create_portfolio(
    name: str, description: str = "", target_return: float = 0.0, risk_tolerance: str = "moderate"
):
    """Create an investment portfolio."""
    portfolio = Portfolio(
        name=name, description=description, target_return=target_return, risk_tolerance=risk_tolerance
    )
    portfolios.append(portfolio)
    logger.info("Portfolio created", portfolio_id=portfolio.id, name=name)
    return portfolio


@app.get("/portfolios", response_model=List[Portfolio])
async def list_portfolios():
    """List all portfolios."""
    return portfolios


@app.post("/portfolios/{portfolio_id}/holdings", response_model=InvestmentHolding)
async def add_holding(
    portfolio_id: str,
    instrument_name: str,
    instrument_type: str,
    quantity: float,
    purchase_price: float,
    current_price: float,
    currency: str = "USD",
    sector: str = "",
):
    """Add a holding to a portfolio."""
    portfolio = next((p for p in portfolios if p.id == portfolio_id), None)
    if not portfolio:
        raise HTTPException(status_code=404, detail="Portfolio not found")

    holding = InvestmentHolding(
        portfolio_id=portfolio_id,
        instrument_name=instrument_name,
        instrument_type=instrument_type,
        quantity=quantity,
        purchase_price=purchase_price,
        current_price=current_price,
        currency=currency,
        sector=sector,
    )
    holdings.append(holding)
    logger.info("Holding added", portfolio_id=portfolio_id, instrument=instrument_name)
    return holding


@app.get("/portfolios/{portfolio_id}/holdings", response_model=List[InvestmentHolding])
async def list_holdings(portfolio_id: str):
    """List holdings in a portfolio."""
    return [h for h in holdings if h.portfolio_id == portfolio_id]


@app.post("/portfolios/{portfolio_id}/performance", response_model=PerformanceMetric)
async def record_performance(portfolio_id: str):
    """Calculate and record portfolio performance metrics."""
    portfolio_holdings = [h for h in holdings if h.portfolio_id == portfolio_id]
    if not portfolio_holdings:
        raise HTTPException(status_code=404, detail="No holdings found for portfolio")

    total_value = sum(h.current_price * h.quantity for h in portfolio_holdings)
    total_cost = sum(h.purchase_price * h.quantity for h in portfolio_holdings)
    unrealized_gain = total_value - total_cost
    unrealized_gain_pct = (unrealized_gain / total_cost * 100) if total_cost > 0 else 0.0

    metric = PerformanceMetric(
        portfolio_id=portfolio_id,
        metric_date=datetime.now(timezone.utc),
        total_value=total_value,
        total_cost=total_cost,
        unrealized_gain=unrealized_gain,
        unrealized_gain_pct=unrealized_gain_pct,
    )
    performance.append(metric)
    logger.info("Performance recorded", portfolio_id=portfolio_id, total_value=total_value)
    return metric


@app.get("/portfolios/{portfolio_id}/performance", response_model=List[PerformanceMetric])
async def list_performance(portfolio_id: str, limit: int = 30):
    """List performance metrics for a portfolio."""
    result = [p for p in performance if p.portfolio_id == portfolio_id]
    return result[-limit:]


@app.post("/holdings/{holding_id}/update-price")
async def update_price(holding_id: str, new_price: float):
    """Update the current price of a holding."""
    holding = next((h for h in holdings if h.id == holding_id), None)
    if not holding:
        raise HTTPException(status_code=404, detail="Holding not found")

    holding.current_price = new_price
    holding.updated_at = datetime.now(timezone.utc)
    logger.info("Price updated", holding_id=holding_id, new_price=new_price)
    return {"holding_id": holding_id, "new_price": new_price}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
