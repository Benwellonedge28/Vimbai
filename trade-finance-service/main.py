"""Vimbai Trade Finance Service - LCs, documentary collections, guarantees, factoring. Port: 8380

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "trade_finance_service" not in _sys.modules or not hasattr(_sys.modules.get("trade_finance_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("trade_finance_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["trade_finance_service"] = _pkg
    _sys.modules["trade_finance_service"].__path__ = [_HERE]

import os
from typing import List

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from trade_finance_service import crud, models
from trade_finance_service.dependencies import book_id_var, get_db_session, get_user_id
from trade_finance_service.exceptions import TradeFinanceError

SERVICE_NAME = "trade-finance-service"
PORT = int(os.getenv("PORT", "8380"))
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
app = FastAPI(title="Vimbai Trade Finance Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.exception_handler(TradeFinanceError)
async def _trade_finance_error(request: Request, exc: TradeFinanceError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/instruments", response_model=models.InstrumentResult)
async def create_instrument(
    inst: models.TradeInstrumentCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_instrument(db_session, user_id, inst)
    logger.info(
        "instrument_created",
        company_id=item.company_id,
        instrument_type=item.instrument_type,
        amount=item.amount,
    )
    return crud.build_result(item)


@app.get("/instruments", response_model=List[models.TradeInstrument])
async def list_instruments(
    company_id: str,
    status: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.list_instruments(db_session, user_id, company_id, status)


@app.post("/instruments/{instrument_id}/present")
async def present_documents(
    instrument_id: str,
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        item = await crud.present_documents(db_session, user_id, company_id, instrument_id)
    except TradeFinanceError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))
    logger.info("instrument_presented", instrument_id=item.id)
    return {"instrument_id": instrument_id, "status": "presented"}


@app.post("/instruments/{instrument_id}/settle")
async def settle_instrument(
    instrument_id: str,
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        item = await crud.settle_instrument(db_session, user_id, company_id, instrument_id)
    except TradeFinanceError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))
    logger.info("instrument_settled", instrument_id=item.id, amount=item.amount)
    return {"instrument_id": instrument_id, "status": "paid", "amount": item.amount}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
