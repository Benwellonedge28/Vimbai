"""
Vimbai Business Combination Service
IFRS 3 acquisition accounting, goodwill calculation, and purchase price allocation.
Port: 8386
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "business-combination-service"
PORT = int(os.getenv("PORT", "8386"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Business Combination Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class IdentifiableAsset(BaseModel):
    name: str
    fair_value: float
    type: str = "tangible"  # tangible, intangible, liability


class AcquisitionRequest(BaseModel):
    company_id: str
    acquirer: str
    acquiree: str
    purchase_price: float
    goodwill_recognized: bool = True
    identifiable_assets: List[IdentifiableAsset] = []
    contingent_consideration: float = 0
    acquisition_costs: float = 0
    non_controlling_interest: float = 0


class AcquisitionResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    acquirer: str
    acquiree: str
    purchase_price: float
    total_identifiable_assets: float
    total_identifiable_liabilities: float
    net_identifiable_assets: float
    goodwill: float
    bargain_purchase: float
    contingent_consideration: float
    acquisition_costs: float
    non_controlling_interest: float
    purchase_price_allocation: List[Dict] = []


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/acquire", response_model=AcquisitionResult)
async def calculate_acquisition(req: AcquisitionRequest):
    total_assets = sum(a.fair_value for a in req.identifiable_assets if a.type in ("tangible", "intangible"))
    total_liabilities = sum(abs(a.fair_value) for a in req.identifiable_assets if a.type == "liability")
    net_identifiable = total_assets - total_liabilities

    consideration = req.purchase_price + req.contingent_consideration + req.non_controlling_interest
    goodwill = consideration - net_identifiable
    bargain = max(0, net_identifiable - consideration)

    allocation = []
    for a in req.identifiable_assets:
        allocation.append(
            {
                "asset": a.name,
                "type": a.type,
                "fair_value": round(a.fair_value, 2),
                "allocation_pct": round(abs(a.fair_value) / max(total_assets, 1) * 100, 1),
            }
        )
    if goodwill > 0 and req.goodwill_recognized:
        allocation.append(
            {"asset": "Goodwill", "type": "intangible", "fair_value": round(goodwill, 2), "allocation_pct": 0}
        )
    if bargain > 0:
        allocation.append(
            {"asset": "Bargain Purchase Gain", "type": "income", "fair_value": round(bargain, 2), "allocation_pct": 0}
        )

    return AcquisitionResult(
        company_id=req.company_id,
        acquirer=req.acquirer,
        acquiree=req.acquiree,
        purchase_price=round(req.purchase_price, 2),
        total_identifiable_assets=round(total_assets, 2),
        total_identifiable_liabilities=round(total_liabilities, 2),
        net_identifiable_assets=round(net_identifiable, 2),
        goodwill=round(goodwill, 2),
        bargain_purchase=round(bargain, 2),
        contingent_consideration=round(req.contingent_consideration, 2),
        acquisition_costs=round(req.acquisition_costs, 2),
        non_controlling_interest=round(req.non_controlling_interest, 2),
        purchase_price_allocation=allocation,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
