"""
Vimbai Investment Monitoring Service
Portfolio holdings, price updates and performance snapshots.

Portfolios, holdings and metrics live in Neo4j, scoped to the caller and the Book.
The file may be imported bare (bracket mounts, uvicorn main:app), so it bootstraps
its own package alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "investment_monitoring_service" not in _sys.modules or not hasattr(
    _sys.modules.get("investment_monitoring_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("investment_monitoring_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["investment_monitoring_service"] = _pkg
    _sys.modules["investment_monitoring_service"].__path__ = [_HERE]

import os
from typing import List

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from investment_monitoring_service import crud
from investment_monitoring_service.dependencies import book_id_var, get_db_session, get_user_id
from investment_monitoring_service.exceptions import NotFoundError
from investment_monitoring_service.models import InvestmentHolding, PerformanceMetric, Portfolio
from neo4j import AsyncSession

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/portfolios", response_model=Portfolio)
async def create_portfolio(
    name: str,
    description: str = "",
    target_return: float = 0.0,
    risk_tolerance: str = "moderate",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create an investment portfolio."""
    portfolio = await crud.create_portfolio(
        db_session,
        user_id,
        name=name,
        description=description,
        target_return=target_return,
        risk_tolerance=risk_tolerance,
    )
    logger.info("Portfolio created", portfolio_id=portfolio.id, name=name)
    return portfolio


@app.get("/portfolios", response_model=List[Portfolio])
async def list_portfolios(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all portfolios."""
    return await crud.list_portfolios(db_session, user_id)


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
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add a holding to a portfolio."""
    try:
        holding = await crud.add_holding(
            db_session,
            user_id,
            portfolio_id,
            instrument_name=instrument_name,
            instrument_type=instrument_type,
            quantity=quantity,
            purchase_price=purchase_price,
            current_price=current_price,
            currency=currency,
            sector=sector,
        )
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Portfolio not found")
    logger.info("Holding added", portfolio_id=portfolio_id, instrument=instrument_name)
    return holding


@app.get("/portfolios/{portfolio_id}/holdings", response_model=List[InvestmentHolding])
async def list_holdings(
    portfolio_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List holdings in a portfolio."""
    return await crud.list_holdings(db_session, user_id, portfolio_id)


@app.post("/portfolios/{portfolio_id}/performance", response_model=PerformanceMetric)
async def record_performance(
    portfolio_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate and record portfolio performance metrics."""
    try:
        metric = await crud.record_performance(db_session, user_id, portfolio_id)
    except NotFoundError:
        raise HTTPException(status_code=404, detail="No holdings found for portfolio")
    logger.info("Performance recorded", portfolio_id=portfolio_id, total_value=metric.total_value)
    return metric


@app.get("/portfolios/{portfolio_id}/performance", response_model=List[PerformanceMetric])
async def list_performance(
    portfolio_id: str,
    limit: int = 30,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List performance metrics for a portfolio."""
    return await crud.list_performance(db_session, user_id, portfolio_id, limit)


@app.post("/holdings/{holding_id}/update-price")
async def update_price(
    holding_id: str,
    new_price: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update the current price of a holding."""
    try:
        result = await crud.update_price(db_session, user_id, holding_id, new_price)
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Holding not found")
    logger.info("Price updated", holding_id=holding_id, new_price=new_price)
    return result


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
