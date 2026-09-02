"""
Vimbai Intangible Assets Service
Manages intangible assets: goodwill, patents, trademarks, copyrights, and amortization schedules.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "intangible-assets-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8420"))

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

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class IntangibleAsset(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    asset_type: str  # goodwill, patent, trademark, copyright, software, license
    cost: float
    useful_life_years: int
    residual_value: float = 0.0
    acquisition_date: datetime
    amortization_method: str = "straight_line"  # straight_line, reducing_balance
    accumulated_amortization: float = 0.0
    net_book_value: float = 0.0
    status: str = "active"  # active, fully_amortized, impaired, disposed
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class AmortizationEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_id: str
    period: str  # YYYY-MM
    amortization_amount: float
    accumulated_amortization: float
    net_book_value: float
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


assets: List[IntangibleAsset] = []
amortization_entries: List[AmortizationEntry] = []


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
):
    """Register a new intangible asset."""
    valid_types = ["goodwill", "patent", "trademark", "copyright", "software", "license"]
    if asset_type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid type. Must be one of {valid_types}")

    asset = IntangibleAsset(
        name=name,
        asset_type=asset_type,
        cost=cost,
        useful_life_years=useful_life_years,
        residual_value=residual_value,
        acquisition_date=acquisition_date,
        amortization_method=amortization_method,
        net_book_value=cost,
    )
    assets.append(asset)
    logger.info("Intangible asset registered", asset_id=asset.id, name=name, type=asset_type)
    return asset


@app.get("/assets", response_model=List[IntangibleAsset])
async def list_assets(asset_type: Optional[str] = None, status: Optional[str] = None):
    """List intangible assets with optional filters."""
    result = assets
    if asset_type:
        result = [a for a in result if a.asset_type == asset_type]
    if status:
        result = [a for a in result if a.status == status]
    return result


@app.get("/assets/{asset_id}", response_model=IntangibleAsset)
async def get_asset(asset_id: str):
    """Get a specific intangible asset."""
    asset = next((a for a in assets if a.id == asset_id), None)
    if not asset:
        raise HTTPException(status_code=404, detail="Asset not found")
    return asset


@app.post("/assets/{asset_id}/amortize", response_model=AmortizationEntry)
async def amortize_asset(asset_id: str, period: str):
    """Record amortization for an asset for a given period."""
    asset = next((a for a in assets if a.id == asset_id), None)
    if not asset:
        raise HTTPException(status_code=404, detail="Asset not found")
    if asset.status != "active":
        raise HTTPException(status_code=400, detail=f"Asset is {asset.status}, cannot amortize")

    if asset.amortization_method == "straight_line":
        annual_amort = (asset.cost - asset.residual_value) / asset.useful_life_years
        monthly_amort = annual_amort / 12
    else:
        monthly_amort = (asset.net_book_value - asset.residual_value) * 0.1 / 12  # simplified reducing balance

    asset.accumulated_amortization += monthly_amort
    asset.net_book_value = asset.cost - asset.accumulated_amortization

    if asset.net_book_value <= asset.residual_value:
        asset.net_book_value = asset.residual_value
        asset.status = "fully_amortized"

    entry = AmortizationEntry(
        asset_id=asset_id,
        period=period,
        amortization_amount=monthly_amort,
        accumulated_amortization=asset.accumulated_amortization,
        net_book_value=asset.net_book_value,
    )
    amortization_entries.append(entry)
    logger.info("Amortization recorded", asset_id=asset_id, period=period, amount=monthly_amort)
    return entry


@app.get("/assets/{asset_id}/amortization", response_model=List[AmortizationEntry])
async def list_amortization(asset_id: str):
    """List amortization entries for an asset."""
    return [e for e in amortization_entries if e.asset_id == asset_id]


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
