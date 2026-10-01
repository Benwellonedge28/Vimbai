"""Vimbai Treasury Management Service - Cash flows, positions, forecasts, investment options. Port: 9003 sub-app

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "treasury_management_service" not in _sys.modules or not hasattr(
    _sys.modules.get("treasury_management_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("treasury_management_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["treasury_management_service"] = _pkg
    _sys.modules["treasury_management_service"].__path__ = [_HERE]

import os

import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from treasury_management_service import crud, models
from treasury_management_service.dependencies import book_id_var, get_db_session, get_user_id
from treasury_management_service.exceptions import TreasuryManagementError
from treasury_management_service.models import InvestmentOption

SERVICE_NAME = "treasury-management-service"
PORT = int(os.getenv("PORT", "8370"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Treasury Management Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.exception_handler(TreasuryManagementError)
async def _treasury_management_error(request: Request, exc: TreasuryManagementError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/cashflows")
async def record_cashflow(
    entry: models.CashFlowEntryCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.record_cashflow(db_session, user_id, entry)
    logger.info("cashflow_recorded", company_id=item.company_id, type=item.flow_type, amount=item.amount)
    return {"id": item.id, "status": "recorded"}


@app.get("/cashflows/{company_id}")
async def get_cashflows(
    company_id: str,
    limit: int = 100,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    flows = await crud.list_cashflows(db_session, user_id, company_id, limit=0)
    recent = await crud.list_cashflows(db_session, user_id, company_id, limit=limit)
    return {
        "company_id": company_id,
        "cashflows": recent,
        "total": len(flows),
    }


@app.get("/position/{company_id}", response_model=models.CashPosition)
async def get_cash_position(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.get_cash_position(db_session, user_id, company_id)


@app.put("/position/{company_id}", response_model=models.CashPosition)
async def update_cash_position(
    company_id: str,
    position: models.CashPositionUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.update_cash_position(db_session, user_id, company_id, position)
    logger.info("cash_position_updated", company_id=company_id, liquidity=item.liquidity_level)
    return item


@app.post("/forecast/{company_id}", response_model=models.CashFlowForecast)
async def generate_forecast(
    company_id: str,
    days: int = 30,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.generate_forecast(db_session, user_id, company_id, days)


@app.get("/investment-options")
async def get_investment_options():
    options = [
        InvestmentOption(
            name="Money Market Fund",
            instrument_type="money_market",
            expected_return=0.045,
            duration_days=30,
            min_amount=1000,
            risk_level="low",
            liquidity="high",
        ),
        InvestmentOption(
            name="Treasury Bill 91-Day",
            instrument_type="treasury_bill",
            expected_return=0.07,
            duration_days=91,
            min_amount=5000,
            risk_level="very_low",
            liquidity="medium",
        ),
        InvestmentOption(
            name="Fixed Deposit 6M",
            instrument_type="fixed_deposit",
            expected_return=0.06,
            duration_days=180,
            min_amount=10000,
            risk_level="very_low",
            liquidity="low",
        ),
        InvestmentOption(
            name="Government Bond 2Y",
            instrument_type="bond",
            expected_return=0.085,
            duration_days=730,
            min_amount=25000,
            risk_level="low",
            liquidity="low",
        ),
    ]
    return {"options": options}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
