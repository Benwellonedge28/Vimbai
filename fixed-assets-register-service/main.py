"""
Vimbai Fixed Assets Register Service
Maintains a comprehensive register of fixed assets with depreciation tracking.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "fixed-assets-register-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8355"))

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

app = FastAPI(title="Vimbai Fixed Assets Register", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class FixedAsset(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_code: str
    asset_name: str
    category: str  # land, buildings, vehicles, machinery, furniture, equipment, IT
    location: str = ""
    department: str = ""
    acquisition_date: datetime
    acquisition_cost: float
    useful_life_years: int
    salvage_value: float = 0.0
    depreciation_method: str = "straight_line"  # straight_line, reducing_balance, units_of_production
    accumulated_depreciation: float = 0.0
    net_book_value: float = 0.0
    status: str = "active"  # active, disposed, impaired, under_construction
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class DepreciationEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_id: str
    period: str  # YYYY-MM
    depreciation_amount: float
    accumulated_depreciation: float
    net_book_value: float
    method: str
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class AssetDisposal(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_id: str
    disposal_date: datetime
    disposal_value: float = 0.0
    disposal_method: str = "sale"  # sale, scrap, donation, write_off
    gain_loss: float = 0.0
    notes: str = ""


assets: List[FixedAsset] = []
depreciation_entries: List[DepreciationEntry] = []
disposals: List[AssetDisposal] = []


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
    assets.append(asset)
    logger.info("Fixed asset registered", asset_id=asset.id, code=asset_code)
    return asset


@app.get("/assets", response_model=List[FixedAsset])
async def list_assets(category: Optional[str] = None, status: Optional[str] = None, department: Optional[str] = None):
    """List fixed assets with optional filters."""
    result = assets
    if category:
        result = [a for a in result if a.category == category]
    if status:
        result = [a for a in result if a.status == status]
    if department:
        result = [a for a in result if a.department == department]
    return result


@app.get("/assets/{asset_id}", response_model=FixedAsset)
async def get_asset(asset_id: str):
    """Get a specific fixed asset."""
    asset = next((a for a in assets if a.id == asset_id), None)
    if not asset:
        raise HTTPException(status_code=404, detail="Asset not found")
    return asset


@app.post("/assets/{asset_id}/depreciate", response_model=DepreciationEntry)
async def depreciate_asset(asset_id: str, period: str):
    """Record depreciation for an asset for a given period."""
    asset = next((a for a in assets if a.id == asset_id), None)
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

    entry = DepreciationEntry(
        asset_id=asset_id,
        period=period,
        depreciation_amount=monthly_depr,
        accumulated_depreciation=asset.accumulated_depreciation,
        net_book_value=asset.net_book_value,
        method=asset.depreciation_method,
    )
    depreciation_entries.append(entry)
    logger.info("Depreciation recorded", asset_id=asset_id, period=period, amount=monthly_depr)
    return entry


@app.get("/assets/{asset_id}/depreciation", response_model=List[DepreciationEntry])
async def list_depreciation(asset_id: str):
    """List depreciation entries for an asset."""
    return [e for e in depreciation_entries if e.asset_id == asset_id]


@app.post("/assets/{asset_id}/dispose", response_model=AssetDisposal)
async def dispose_asset(
    asset_id: str, disposal_date: datetime, disposal_value: float = 0.0, disposal_method: str = "sale", notes: str = ""
):
    """Dispose of a fixed asset."""
    asset = next((a for a in assets if a.id == asset_id), None)
    if not asset:
        raise HTTPException(status_code=404, detail="Asset not found")
    if asset.status == "disposed":
        raise HTTPException(status_code=400, detail="Asset already disposed")

    gain_loss = disposal_value - asset.net_book_value
    asset.status = "disposed"

    disposal = AssetDisposal(
        asset_id=asset_id,
        disposal_date=disposal_date,
        disposal_value=disposal_value,
        disposal_method=disposal_method,
        gain_loss=gain_loss,
        notes=notes,
    )
    disposals.append(disposal)
    logger.info("Asset disposed", asset_id=asset_id, method=disposal_method, gain_loss=gain_loss)
    return disposal


@app.get("/summary")
async def asset_summary():
    """Get fixed asset register summary."""
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
