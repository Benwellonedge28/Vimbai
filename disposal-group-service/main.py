"""
Vimbai Disposal Group Service
IFRS 5 held-for-sale disposal groups, their assets, and impairment testing.

Groups, assets and impairment tests live in Neo4j, scoped to the caller and the
Book. The file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "disposal_group_service" not in _sys.modules or not hasattr(_sys.modules.get("disposal_group_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("disposal_group_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["disposal_group_service"] = _pkg
    _sys.modules["disposal_group_service"].__path__ = [_HERE]

import os
from datetime import datetime
from typing import List, Optional

import structlog
from disposal_group_service import crud
from disposal_group_service.dependencies import book_id_var, get_db_session, get_user_id
from disposal_group_service.exceptions import NotFoundError
from disposal_group_service.models import DisposalAsset, DisposalGroup, ImpairmentTest
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "disposal-group-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8226"))

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

app = FastAPI(title="Vimbai Disposal Group Service", version=SERVICE_VERSION, docs_url="/docs")
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


@app.post("/groups", response_model=DisposalGroup)
async def create_group(
    name: str,
    description: str = "",
    disposal_method: str = "sale",
    expected_disposal_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a disposal group (held-for-sale classification)."""
    group = await crud.create_group(
        db_session,
        user_id,
        name=name,
        description=description,
        disposal_method=disposal_method,
        expected_disposal_date=expected_disposal_date,
    )
    logger.info("Disposal group created", group_id=group.id, name=name)
    return group


@app.get("/groups", response_model=List[DisposalGroup])
async def list_groups(
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List disposal groups."""
    return await crud.list_groups(db_session, user_id, status)


@app.post("/groups/{group_id}/assets", response_model=DisposalAsset)
async def add_asset(
    group_id: str,
    asset_id: str,
    asset_name: str,
    carrying_amount: float,
    fair_value: float = 0.0,
    classification: str = "noncurrent_asset",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add an asset to a disposal group."""
    try:
        asset = await crud.add_asset(
            db_session,
            user_id,
            group_id,
            asset_id=asset_id,
            asset_name=asset_name,
            carrying_amount=carrying_amount,
            fair_value=fair_value,
            classification=classification,
        )
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Disposal group not found")
    logger.info("Asset added to disposal group", group_id=group_id, asset_id=asset_id)
    return asset


@app.get("/groups/{group_id}/assets", response_model=List[DisposalAsset])
async def list_assets(
    group_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List assets in a disposal group."""
    return await crud.list_assets(db_session, user_id, group_id)


@app.post("/groups/{group_id}/impairment-test", response_model=ImpairmentTest)
async def test_impairment(
    group_id: str,
    fair_value_less_costs: float,
    notes: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Perform impairment test on disposal group (IFRS 5)."""
    try:
        test = await crud.test_impairment(db_session, user_id, group_id, fair_value_less_costs, notes)
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Disposal group not found")
    logger.info("Impairment test completed", group_id=group_id, impairment=test.impairment_amount)
    return test


@app.put("/groups/{group_id}/status")
async def update_status(
    group_id: str,
    status: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update disposal group status."""
    if status not in crud.VALID_STATUSES:
        raise HTTPException(status_code=400, detail=f"Invalid status. Must be one of {crud.VALID_STATUSES}")

    try:
        return await crud.update_status(db_session, user_id, group_id, status)
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Disposal group not found")


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
