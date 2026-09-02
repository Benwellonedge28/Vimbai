"""
Intangible Assets Service
Port: 8358
Intangible assets tracking and impairment testing
"""

from datetime import date, datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Intangible Assets Service", version="1.0.0")


class IntangibleAssetRequest(BaseModel):
    company_id: str
    assets: List[Dict[str, Any]]
    testing_date: date


class IntangibleAssetResponse(BaseModel):
    company_id: str
    total_cost: float
    total_accum_amort: float
    total_nbv: float
    impairment_charges: float
    assets_detail: List[Dict[str, Any]]


class ImpairmentTestRequest(BaseModel):
    asset_id: str
    carrying_value: float
    recoverable_amount: float
    cash_generating_unit: str


class ImpairmentTestResponse(BaseModel):
    asset_id: str
    carrying_value: float
    recoverable_amount: float
    impairment_loss: float
    impaired: bool


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "intangible-assets", "version": "1.0.0"}


@app.post("/assets", response_model=IntangibleAssetResponse)
async def get_intangible_assets(request: IntangibleAssetRequest):
    logger.info("Getting intangible assets", company=request.company_id)

    total_cost = sum(a.get("cost", 0) for a in request.assets)
    total_accum = sum(a.get("accumulated_amortization", 0) for a in request.assets)

    return IntangibleAssetResponse(
        company_id=request.company_id,
        total_cost=round(total_cost, 2),
        total_accum_amort=round(total_accum, 2),
        total_nbv=round(total_cost - total_accum, 2),
        impairment_charges=0.0,
        assets_detail=request.assets,
    )


@app.post("/impairment-test", response_model=ImpairmentTestResponse)
async def test_impairment(request: ImpairmentTestRequest):
    logger.info("Testing impairment", asset=request.asset_id)

    impairment = max(0, request.carrying_value - request.recoverable_amount)

    return ImpairmentTestResponse(
        asset_id=request.asset_id,
        carrying_value=request.carrying_value,
        recoverable_amount=request.recoverable_amount,
        impairment_loss=round(impairment, 2),
        impaired=impairment > 0,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8358)
