"""Vimbai Revenue Recognition Service - IFRS 15 contract revenue tracking. Port: 8370

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "revenue_recognition_service" not in _sys.modules or not hasattr(
    _sys.modules.get("revenue_recognition_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("revenue_recognition_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["revenue_recognition_service"] = _pkg
    _sys.modules["revenue_recognition_service"].__path__ = [_HERE]

import os

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from revenue_recognition_service import crud, models
from revenue_recognition_service.dependencies import book_id_var, get_db_session, get_user_id
from revenue_recognition_service.exceptions import RevenueRecognitionError

SERVICE_NAME = "revenue-recognition-service"
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
app = FastAPI(title="Vimbai Revenue Recognition Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.exception_handler(RevenueRecognitionError)
async def _revenue_recognition_error(request: Request, exc: RevenueRecognitionError):
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


@app.post("/contracts", response_model=models.RevenueContract)
async def create_contract(
    contract: models.RevenueContractCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_contract(db_session, user_id, contract)
    logger.info(
        "contract_created",
        company_id=item.company_id,
        customer=item.customer_name,
        value=item.total_transaction_price,
    )
    return item


@app.post("/contracts/{contract_id}/recognize")
async def recognize_revenue(
    contract_id: str,
    obligation_id: str,
    amount: float = 0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.recognize_revenue(db_session, user_id, contract_id, obligation_id, amount)
    except RevenueRecognitionError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.get("/contracts/{company_id}")
async def get_contracts(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    contracts = await crud.list_contracts(db_session, user_id, company_id)
    return {"company_id": company_id, "contracts": contracts, "total": len(contracts)}


@app.get("/summary/{company_id}")
async def revenue_summary(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.revenue_summary(db_session, user_id, company_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
