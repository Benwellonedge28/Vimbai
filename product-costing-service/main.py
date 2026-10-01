"""Vimbai Product Costing Service - costing analysis and calculation. Port: 8341

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "product_costing_service" not in _sys.modules or not hasattr(
    _sys.modules.get("product_costing_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("product_costing_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["product_costing_service"] = _pkg
    _sys.modules["product_costing_service"].__path__ = [_HERE]

import os

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from product_costing_service import crud, models
from product_costing_service.dependencies import book_id_var, get_db_session, get_user_id
from product_costing_service.exceptions import ProductCostingError

SERVICE_NAME = "product-costing-service"
PORT = int(os.getenv("PORT", "8341"))
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
app = FastAPI(title="Vimbai Product Costing Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.exception_handler(ProductCostingError)
async def _product_costing_error(request: Request, exc: ProductCostingError):
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


@app.post("/calculate", response_model=models.CostCalculation)
async def calculate_cost(
    calc: models.CostCalculationCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_calculation(db_session, user_id, calc)
    logger.info(
        "cost_calculated",
        company_id=item.company_id,
        product=item.product_or_process,
        total=item.total_cost,
        unit=item.unit_cost,
    )
    return item


@app.get("/calculations/{company_id}")
async def get_calculations(
    company_id: str,
    product: str = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.get_calculations(db_session, user_id, company_id, product or "")


@app.get("/breakdown/{company_id}/{calc_id}")
async def get_cost_breakdown(
    company_id: str,
    calc_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.get_cost_breakdown(db_session, user_id, company_id, calc_id)
    except ProductCostingError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.get("/summary/{company_id}")
async def cost_summary(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.cost_summary(db_session, user_id, company_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
