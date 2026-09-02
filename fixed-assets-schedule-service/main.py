"""
Vimbai Fixed Assets Schedule Service
Manages fixed asset register and schedules.
"""

import os
import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

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


class AssetCategory(str, Enum):
    LAND = "land"
    BUILDINGS = "buildings"
    PLANT_MACHINERY = "plant_machinery"
    FURNITURE_FIXTURES = "furniture_fixtures"
    MOTOR_VEHICLES = "motor_vehicles"
    OFFICE_EQUIPMENT = "office_equipment"
    COMPUTER_EQUIPMENT = "computer_equipment"
    LEASEHOLD_IMPROVEMENTS = "leasehold_improvements"
    INTANGIBLE_ASSETS = "intangible_assets"


class AssetStatus(str, Enum):
    IN_USE = "in_use"
    UNDER_MAINTENANCE = "under_maintenance"
    IDLE = "idle"
    DISPOSED = "disposed"
    SCRAPPED = "scrapped"


class FixedAsset(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_code: str
    asset_name: str
    description: Optional[str] = None
    category: AssetCategory
    location: Optional[str] = None
    custodian: Optional[str] = None
    purchase_date: datetime
    purchase_cost: float
    residual_value: float = 0
    useful_life_years: int
    depreciation_method: str = "straight_line"
    accumulated_depreciation: float = 0
    net_book_value: float = 0
    status: AssetStatus = AssetStatus.IN_USE
    supplier: Optional[str] = None
    warranty_expiry: Optional[datetime] = None
    insurance_policy: Optional[str] = None
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)


class AssetSchedule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    as_of_date: datetime
    assets: List[FixedAsset] = []
    total_cost: float = 0
    total_accumulated_depreciation: float = 0
    total_net_book_value: float = 0
    category_summary: Dict[str, Dict[str, float]] = {}
    created_at: datetime = Field(default_factory=datetime.utcnow)


# In-memory storage
fixed_assets: Dict[str, FixedAsset] = {}


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
async def register_asset(data: FixedAsset):
    """Register a new fixed asset."""
    data.id = str(uuid.uuid4())
    data.net_book_value = data.purchase_cost - data.accumulated_depreciation
    data.created_at = datetime.utcnow()
    data.updated_at = datetime.utcnow()
    fixed_assets[data.id] = data
    await call_audit_service("CREATE", "asset", data.id, {"code": data.asset_code, "name": data.asset_name})
    return data


@app.get("/assets")
async def list_assets(category: Optional[AssetCategory] = None, status: Optional[AssetStatus] = None):
    """List all assets."""
    result = list(fixed_assets.values())
    if category:
        result = [a for a in result if a.category == category]
    if status:
        result = [a for a in result if a.status == status]
    return {"assets": result, "count": len(result)}


@app.get("/assets/{asset_id}")
async def get_asset(asset_id: str):
    """Get asset details."""
    asset = fixed_assets.get(asset_id)
    if not asset:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Asset not found")
    return asset


@app.put("/assets/{asset_id}")
async def update_asset(asset_id: str, data: Dict[str, Any]):
    """Update asset details."""
    asset = fixed_assets.get(asset_id)
    if not asset:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Asset not found")
    for key, value in data.items():
        if hasattr(asset, key) and key not in ["id", "created_at"]:
            setattr(asset, key, value)
    asset.updated_at = datetime.utcnow()
    return asset


@app.post("/schedule/generate")
async def generate_schedule(as_of_date: datetime):
    """Generate fixed assets schedule."""
    total_cost = sum(a.purchase_cost for a in fixed_assets.values())
    total_accumulated = sum(a.accumulated_depreciation for a in fixed_assets.values())
    total_nbv = sum(a.net_book_value for a in fixed_assets.values())

    # Category summary
    category_summary = {}
    for category in AssetCategory:
        assets = [a for a in fixed_assets.values() if a.category == category]
        if assets:
            category_summary[category.value] = {
                "count": len(assets),
                "cost": sum(a.purchase_cost for a in assets),
                "accumulated": sum(a.accumulated_depreciation for a in assets),
                "nbv": sum(a.net_book_value for a in assets),
            }

    schedule = AssetSchedule(
        as_of_date=as_of_date,
        assets=list(fixed_assets.values()),
        total_cost=total_cost,
        total_accumulated_depreciation=total_accumulated,
        total_net_book_value=total_nbv,
        category_summary=category_summary,
    )

    await call_audit_service("GENERATE", "schedule", schedule.id, {"asset_count": len(fixed_assets)})
    return schedule


@app.get("/schedule/latest")
async def get_latest_schedule():
    """Get latest schedule."""
    return await generate_schedule(datetime.utcnow())


@app.get("/summary")
async def get_asset_summary():
    """Get asset summary by category."""
    summary = {}
    for category in AssetCategory:
        assets = [a for a in fixed_assets.values() if a.category == category]
        if assets:
            summary[category.value] = {
                "count": len(assets),
                "cost": sum(a.purchase_cost for a in assets),
                "accumulated": sum(a.accumulated_depreciation for a in assets),
                "nbv": sum(a.net_book_value for a in assets),
            }

    return {
        "total_assets": len(fixed_assets),
        "total_cost": sum(a.purchase_cost for a in fixed_assets.values()),
        "total_accumulated_depreciation": sum(a.accumulated_depreciation for a in fixed_assets.values()),
        "total_net_book_value": sum(a.net_book_value for a in fixed_assets.values()),
        "by_category": summary,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
