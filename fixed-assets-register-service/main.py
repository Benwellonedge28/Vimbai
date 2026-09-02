"""
Fixed Assets Register Service
Port: 8355
Asset registry and tracking
"""

from datetime import date, datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Fixed Assets Register Service", version="1.0.0")


class AssetRequest(BaseModel):
    company_id: str
    assets: List[Dict[str, Any]]


class AssetResponse(BaseModel):
    company_id: str
    total_assets: int
    total_cost: float
    total_accumulated_depr: float
    total_net_book_value: float
    assets_by_category: Dict[str, Dict[str, float]]


class AssetTransferRequest(BaseModel):
    asset_id: str
    from_location: str
    to_location: str
    transfer_date: date
    responsible_person: str


class AssetResponse_2(BaseModel):
    asset_id: str
    status: str
    transfer_date: date


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "fixed-assets-register", "version": "1.0.0"}


@app.post("/register", response_model=AssetResponse)
async def register_assets(request: AssetRequest):
    logger.info("Registering assets", company=request.company_id, count=len(request.assets))

    by_category = {}
    total_cost = 0.0
    total_accum = 0.0

    for asset in request.assets:
        cost = asset.get("cost", 0)
        accum = asset.get("accumulated_depreciation", 0)
        cat = asset.get("category", "Other")
        total_cost += cost
        total_accum += accum

        if cat not in by_category:
            by_category[cat] = {"count": 0, "cost": 0, "nbv": 0}
        by_category[cat]["count"] += 1
        by_category[cat]["cost"] += cost
        by_category[cat]["nbv"] += cost - accum

    return AssetResponse(
        company_id=request.company_id,
        total_assets=len(request.assets),
        total_cost=round(total_cost, 2),
        total_accumulated_depr=round(total_accum, 2),
        total_net_book_value=round(total_cost - total_accum, 2),
        assets_by_category={
            k: {"count": v["count"], "cost": round(v["cost"], 2), "nbv": round(v["nbv"], 2)}
            for k, v in by_category.items()
        },
    )


@app.post("/transfer", response_model=AssetResponse_2)
async def transfer_asset(request: AssetTransferRequest):
    logger.info("Transferring asset", asset=request.asset_id)

    return AssetResponse_2(asset_id=request.asset_id, status="transferred", transfer_date=request.transfer_date)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8355)
