"""
Vimbai Disposal Group Service
Manages disposal groups for held-for-sale classification under IFRS 5.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

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

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class DisposalAsset(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_id: str
    asset_name: str
    carrying_amount: float
    fair_value: float = 0.0
    classification: str  # noncurrent_asset, subsidiary, business_segment


class DisposalGroup(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    disposal_method: str = "sale"  # sale, abandonment, spin_off, exchange
    status: str = "held_for_sale"  # held_for_sale, sold, withdrawn
    classification_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    expected_disposal_date: Optional[datetime] = None
    total_carrying_amount: float = 0.0
    total_fair_value: float = 0.0
    impairment_loss: float = 0.0
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ImpairmentTest(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    group_id: str
    carrying_amount: float
    fair_value_less_costs: float
    impairment_amount: float
    tested_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    notes: str = ""


disposal_groups: List[DisposalGroup] = []
disposal_assets: Dict[str, List[DisposalAsset]] = {}
impairment_tests: List[ImpairmentTest] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/groups", response_model=DisposalGroup)
async def create_group(
    name: str, description: str = "", disposal_method: str = "sale", expected_disposal_date: Optional[datetime] = None
):
    """Create a disposal group (held-for-sale classification)."""
    group = DisposalGroup(
        name=name,
        description=description,
        disposal_method=disposal_method,
        expected_disposal_date=expected_disposal_date,
    )
    disposal_groups.append(group)
    disposal_assets[group.id] = []
    logger.info("Disposal group created", group_id=group.id, name=name)
    return group


@app.get("/groups", response_model=List[DisposalGroup])
async def list_groups(status: Optional[str] = None):
    """List disposal groups."""
    if status:
        return [g for g in disposal_groups if g.status == status]
    return disposal_groups


@app.post("/groups/{group_id}/assets", response_model=DisposalAsset)
async def add_asset(
    group_id: str,
    asset_id: str,
    asset_name: str,
    carrying_amount: float,
    fair_value: float = 0.0,
    classification: str = "noncurrent_asset",
):
    """Add an asset to a disposal group."""
    group = next((g for g in disposal_groups if g.id == group_id), None)
    if not group:
        raise HTTPException(status_code=404, detail="Disposal group not found")

    asset = DisposalAsset(
        asset_id=asset_id,
        asset_name=asset_name,
        carrying_amount=carrying_amount,
        fair_value=fair_value,
        classification=classification,
    )
    disposal_assets[group_id].append(asset)
    group.total_carrying_amount += carrying_amount
    group.total_fair_value += fair_value
    logger.info("Asset added to disposal group", group_id=group_id, asset_id=asset_id)
    return asset


@app.get("/groups/{group_id}/assets", response_model=List[DisposalAsset])
async def list_assets(group_id: str):
    """List assets in a disposal group."""
    return disposal_assets.get(group_id, [])


@app.post("/groups/{group_id}/impairment-test", response_model=ImpairmentTest)
async def test_impairment(group_id: str, fair_value_less_costs: float, notes: str = ""):
    """Perform impairment test on disposal group (IFRS 5)."""
    group = next((g for g in disposal_groups if g.id == group_id), None)
    if not group:
        raise HTTPException(status_code=404, detail="Disposal group not found")

    impairment = max(0, group.total_carrying_amount - fair_value_less_costs)
    group.impairment_loss = impairment

    test = ImpairmentTest(
        group_id=group_id,
        carrying_amount=group.total_carrying_amount,
        fair_value_less_costs=fair_value_less_costs,
        impairment_amount=impairment,
        notes=notes,
    )
    impairment_tests.append(test)
    logger.info("Impairment test completed", group_id=group_id, impairment=impairment)
    return test


@app.put("/groups/{group_id}/status")
async def update_status(group_id: str, status: str):
    """Update disposal group status."""
    valid_statuses = ["held_for_sale", "sold", "withdrawn"]
    if status not in valid_statuses:
        raise HTTPException(status_code=400, detail=f"Invalid status. Must be one of {valid_statuses}")

    group = next((g for g in disposal_groups if g.id == group_id), None)
    if not group:
        raise HTTPException(status_code=404, detail="Disposal group not found")

    group.status = status
    return {"group_id": group_id, "status": status}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
