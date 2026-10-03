"""
Vimbai Fixed Assets Register Service
Fixed asset register with depreciation and disposals.

Assets, depreciation entries, and disposals persist in Neo4j, stamped with
the caller (X-User-Id) and the Book context (X-Book-ID, verified upstream
by the API gateway). All lookups and summaries are scoped to the caller's
own Book-visible records. Original status codes (200/400/404) and
depreciation semantics are preserved.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
from datetime import datetime, timezone
from typing import Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "fixed_assets_register_service" not in _sys.modules or not hasattr(
    _sys.modules.get("fixed_assets_register_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("fixed_assets_register_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["fixed_assets_register_service"] = _pkg
    _sys.modules["fixed_assets_register_service"].__path__ = [_HERE]

from typing import List

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from fixed_assets_register_service import crud
from fixed_assets_register_service.database import Neo4jConnector
from fixed_assets_register_service.dependencies import book_id_var, get_db_session, get_user_id
from fixed_assets_register_service.exceptions import FixedAssetsRegisterError
from fixed_assets_register_service.models import AssetDisposal, DepreciationEntry, FixedAsset
from neo4j import AsyncSession

SERVICE_NAME = "fixed-assets-register-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8265"))

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

app = FastAPI(title="Vimbai Fixed Assets Register Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(FixedAssetsRegisterError)
async def _far_error(request: Request, exc: FixedAssetsRegisterError):
    return JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/assets", response_model=FixedAsset)
async def register_asset(
    asset_code: str,
    asset_name: str,
    category: str,
    acquisition_date: datetime,
    acquisition_cost: float,
    useful_life_years: int,
    salvage_value: float = 0.0,
    depreciation_method: str = "straight_line",
    location: str = "",
    department: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Register a fixed asset."""
    valid_cats = ["land", "buildings", "vehicles", "machinery", "furniture", "equipment", "IT"]
    if category not in valid_cats:
        raise HTTPException(status_code=400, detail=f"Invalid category. Must be one of {valid_cats}")

    asset = FixedAsset(
        asset_code=asset_code,
        asset_name=asset_name,
        category=category,
        location=location,
        department=department,
        acquisition_date=acquisition_date,
        acquisition_cost=acquisition_cost,
        useful_life_years=useful_life_years,
        salvage_value=salvage_value,
        depreciation_method=depreciation_method,
        net_book_value=acquisition_cost,
    )
    created = await crud.create_asset(db_session, user_id, asset)
    logger.info("Fixed asset registered", asset_id=created.id, code=asset_code)
    return created


@app.get("/assets", response_model=List[FixedAsset])
async def list_assets(
    category: Optional[str] = None,
    status: Optional[str] = None,
    department: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's fixed assets with optional filters."""
    result = await crud.list_assets(db_session, user_id)
    if category:
        result = [a for a in result if a.category == category]
    if status:
        result = [a for a in result if a.status == status]
    if department:
        result = [a for a in result if a.department == department]
    return result


@app.get("/assets/{asset_id}", response_model=FixedAsset)
async def get_asset(
    asset_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific fixed asset (caller-scoped)."""
    asset = await crud.get_asset(db_session, user_id, asset_id)
    if not asset:
        raise HTTPException(status_code=404, detail="Asset not found")
    return asset


@app.post("/assets/{asset_id}/depreciate", response_model=DepreciationEntry)
async def depreciate_asset(
    asset_id: str,
    period: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record depreciation for an asset for a given period."""
    asset = await crud.get_asset(db_session, user_id, asset_id)
    if not asset:
        raise HTTPException(status_code=404, detail="Asset not found")
    if asset.status != "active":
        raise HTTPException(status_code=400, detail=f"Asset is {asset.status}")

    depreciable_base = asset.acquisition_cost - asset.salvage_value
    if asset.depreciation_method == "straight_line":
        monthly_depr = depreciable_base / (asset.useful_life_years * 12) if asset.useful_life_years > 0 else 0
    elif asset.depreciation_method == "reducing_balance":
        monthly_depr = asset.net_book_value * 0.2 / 12  # 20% annual reducing balance
    else:
        monthly_depr = 0.0

    asset.accumulated_depreciation += monthly_depr
    asset.net_book_value = asset.acquisition_cost - asset.accumulated_depreciation

    if asset.net_book_value <= asset.salvage_value:
        asset.net_book_value = asset.salvage_value
        asset.status = "disposed"

    await crud.update_asset_state(
        db_session,
        user_id,
        asset_id,
        asset.accumulated_depreciation,
        asset.net_book_value,
        asset.status,
    )

    entry = DepreciationEntry(
        asset_id=asset_id,
        period=period,
        depreciation_amount=monthly_depr,
        accumulated_depreciation=asset.accumulated_depreciation,
        net_book_value=asset.net_book_value,
        method=asset.depreciation_method,
    )
    created = await crud.create_entry(db_session, user_id, entry)
    logger.info("Depreciation recorded", asset_id=asset_id, period=period, amount=monthly_depr)
    return created


@app.get("/assets/{asset_id}/depreciation", response_model=List[DepreciationEntry])
async def list_depreciation(
    asset_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List depreciation entries for the caller's asset."""
    return await crud.list_entries(db_session, user_id, asset_id)


@app.post("/assets/{asset_id}/dispose", response_model=AssetDisposal)
async def dispose_asset(
    asset_id: str,
    disposal_date: datetime,
    disposal_value: float = 0.0,
    disposal_method: str = "sale",
    notes: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Dispose of a fixed asset."""
    asset = await crud.get_asset(db_session, user_id, asset_id)
    if not asset:
        raise HTTPException(status_code=404, detail="Asset not found")
    if asset.status == "disposed":
        raise HTTPException(status_code=400, detail="Asset already disposed")

    gain_loss = disposal_value - asset.net_book_value
    await crud.update_asset_state(
        db_session,
        user_id,
        asset_id,
        asset.accumulated_depreciation,
        asset.net_book_value,
        "disposed",
    )

    disposal = AssetDisposal(
        asset_id=asset_id,
        disposal_date=disposal_date,
        disposal_value=disposal_value,
        disposal_method=disposal_method,
        gain_loss=gain_loss,
        notes=notes,
    )
    created = await crud.create_disposal(db_session, user_id, disposal)
    logger.info("Asset disposed", asset_id=asset_id, method=disposal_method, gain_loss=gain_loss)
    return created


@app.get("/summary")
async def asset_summary(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's fixed asset register summary."""
    assets = await crud.list_assets(db_session, user_id)
    return {
        "total_assets": len(assets),
        "active_assets": len([a for a in assets if a.status == "active"]),
        "total_acquisition_cost": sum(a.acquisition_cost for a in assets),
        "total_accumulated_depreciation": sum(a.accumulated_depreciation for a in assets),
        "total_net_book_value": sum(a.net_book_value for a in assets),
        "by_category": {
            cat: {
                "count": len([a for a in assets if a.category == cat]),
                "nbv": sum(a.net_book_value for a in assets if a.category == cat),
            }
            for cat in set(a.category for a in assets)
        },
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
