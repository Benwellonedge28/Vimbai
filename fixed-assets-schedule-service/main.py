"""
Vimbai Fixed Assets Schedule Service
Fixed asset register and schedules.

The register lives in Neo4j, scoped to the caller and the Book. The file may be
imported bare (bracket mounts, uvicorn main:app), so it bootstraps its own package
alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "fixed_assets_schedule_service" not in _sys.modules or not hasattr(
    _sys.modules.get("fixed_assets_schedule_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("fixed_assets_schedule_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["fixed_assets_schedule_service"] = _pkg
    _sys.modules["fixed_assets_schedule_service"].__path__ = [_HERE]

import os
from datetime import datetime
from typing import Any, Dict, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, HTTPException, Request, status
from fastapi.middleware.cors import CORSMiddleware
from fixed_assets_schedule_service import crud
from fixed_assets_schedule_service.dependencies import book_id_var, get_db_session, get_user_id
from fixed_assets_schedule_service.exceptions import NotFoundError
from fixed_assets_schedule_service.models import (
    AssetCategory,
    AssetSchedule,
    AssetStatus,
    FixedAsset,
)
from neo4j import AsyncSession

SERVICE_NAME = "fixed-assets-schedule-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8037"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

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

app = FastAPI(title="Vimbai Fixed Assets Schedule Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


async def call_audit_service(action: str, resource_type: str, resource_id: str, details: Dict[str, Any]):
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            await client.post(
                f"{AUDIT_SERVICE_URL}/audit",
                json={
                    "action": action,
                    "resource_type": resource_type,
                    "resource_id": resource_id,
                    "details": details,
                    "timestamp": datetime.utcnow().isoformat(),
                },
            )
    except Exception:
        pass


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Fixed asset register and schedules"}


@app.post("/assets", response_model=FixedAsset, status_code=status.HTTP_201_CREATED)
async def register_asset(
    data: FixedAsset,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Register a new fixed asset."""
    asset = await crud.register_asset(db_session, user_id, data)
    await call_audit_service("CREATE", "asset", asset.id, {"code": asset.asset_code, "name": asset.asset_name})
    return asset


@app.get("/assets")
async def list_assets(
    category: Optional[AssetCategory] = None,
    status: Optional[AssetStatus] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all assets."""
    assets = await crud.list_assets(db_session, user_id, category, status)
    return {"assets": assets, "count": len(assets)}


@app.get("/assets/{asset_id}")
async def get_asset(
    asset_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get asset details."""
    try:
        return await crud.get_asset(db_session, user_id, asset_id)
    except NotFoundError:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Asset not found")


@app.put("/assets/{asset_id}")
async def update_asset(
    asset_id: str,
    data: Dict[str, Any],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update asset details."""
    try:
        return await crud.update_asset(db_session, user_id, asset_id, data)
    except NotFoundError:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Asset not found")


@app.post("/schedule/generate")
async def generate_schedule(
    as_of_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Generate fixed assets schedule."""
    schedule = await crud.generate_schedule(db_session, user_id, as_of_date)
    await call_audit_service("GENERATE", "schedule", schedule.id, {"asset_count": len(schedule.assets)})
    return schedule


@app.get("/schedule/latest")
async def get_latest_schedule(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get latest schedule."""
    return await crud.generate_schedule(db_session, user_id, datetime.utcnow())


@app.get("/summary")
async def get_asset_summary(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get asset summary by category."""
    return await crud.summarise(db_session, user_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
