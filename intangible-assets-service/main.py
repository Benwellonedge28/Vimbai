"""
Vimbai Intangible Assets Service
Intangible asset register with amortisation. Port: 8037

Assets and their amortisation history live in Neo4j, scoped to the caller and the
Book. The file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "intangible_assets_service" not in _sys.modules or not hasattr(
    _sys.modules.get("intangible_assets_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("intangible_assets_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["intangible_assets_service"] = _pkg
    _sys.modules["intangible_assets_service"].__path__ = [_HERE]

import os
from datetime import datetime
from typing import List, Optional

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from intangible_assets_service import crud
from intangible_assets_service.dependencies import book_id_var, get_db_session, get_user_id
from intangible_assets_service.exceptions import NotFoundError
from intangible_assets_service.models import AmortizationEntry, IntangibleAsset
from neo4j import AsyncSession

SERVICE_NAME = "intangible-assets-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8358"))

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

app = FastAPI(title="Vimbai Intangible Assets Service", version=SERVICE_VERSION, docs_url="/docs")
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


@app.post("/assets", response_model=IntangibleAsset)
async def create_asset(
    name: str,
    asset_type: str,
    cost: float,
    useful_life_years: int,
    acquisition_date: datetime,
    residual_value: float = 0.0,
    amortization_method: str = "straight_line",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Register a new intangible asset."""
    if asset_type not in crud.VALID_TYPES:
        raise HTTPException(status_code=400, detail=f"Invalid type. Must be one of {crud.VALID_TYPES}")

    asset = await crud.create_asset(
        db_session,
        user_id,
        name=name,
        asset_type=asset_type,
        cost=cost,
        useful_life_years=useful_life_years,
        acquisition_date=acquisition_date,
        residual_value=residual_value,
        amortization_method=amortization_method,
    )
    logger.info("Intangible asset registered", asset_id=asset.id, name=name, type=asset_type)
    return asset


@app.get("/assets", response_model=List[IntangibleAsset])
async def list_assets(
    asset_type: Optional[str] = None,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List intangible assets with optional filters."""
    return await crud.list_assets(db_session, user_id, asset_type, status)


@app.get("/assets/{asset_id}", response_model=IntangibleAsset)
async def get_asset(
    asset_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific intangible asset."""
    try:
        return await crud.get_asset(db_session, user_id, asset_id)
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Asset not found")


@app.post("/assets/{asset_id}/amortize", response_model=AmortizationEntry)
async def amortize_asset(
    asset_id: str,
    period: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record amortization for an asset for a given period."""
    try:
        asset = await crud.get_asset(db_session, user_id, asset_id)
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Asset not found")

    if asset.status != "active":
        raise HTTPException(status_code=400, detail=f"Asset is {asset.status}, cannot amortize")

    entry = await crud.amortize_asset(db_session, user_id, asset_id, period)
    logger.info(
        "Amortization recorded",
        asset_id=asset_id,
        period=period,
        amount=entry.amortization_amount,
    )
    return entry


@app.get("/assets/{asset_id}/amortization", response_model=List[AmortizationEntry])
async def list_amortization(
    asset_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List amortization entries for an asset."""
    return await crud.list_amortization(db_session, user_id, asset_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
