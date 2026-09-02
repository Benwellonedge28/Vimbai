"""
Vimbai Make-or-Buy Decision Service
Cost analysis for in-house production vs outsourcing decisions.
Port: 8345
"""

import os
import uuid
from typing import Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "make-or-buy-decision-service"
PORT = int(os.getenv("PORT", "8345"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Make-or-Buy Decision Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class MakeCost(BaseModel):
    direct_materials: float
    direct_labour: float
    variable_overhead: float
    fixed_overhead: float
    setup_cost: float = 0
    tooling_cost: float = 0


class BuyCost(BaseModel):
    unit_purchase_price: float
    ordering_cost: float = 0
    carrying_cost_per_unit: float = 0
    quality_inspection_cost: float = 0


class MakeOrBuyRequest(BaseModel):
    company_id: str
    product_name: str
    annual_volume: int
    make_costs: MakeCost
    buy_costs: BuyCost
    opportunity_cost: float = 0
    quality_considerations: str = ""


class MakeOrBuyResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    product_name: str
    annual_volume: int
    make_total_cost: float
    buy_total_cost: float
    difference: float
    recommendation: str
    make_cost_per_unit: float
    buy_cost_per_unit: float
    qualitative_factors: List[str] = []


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/analyze", response_model=MakeOrBuyResult)
async def analyze_make_or_buy(req: MakeOrBuyRequest):
    vol = req.annual_volume
    make_total = (
        (req.make_costs.direct_materials + req.make_costs.direct_labour + req.make_costs.variable_overhead) * vol
        + req.make_costs.fixed_overhead
        + req.make_costs.setup_cost
        + req.make_costs.tooling_cost
        + req.opportunity_cost
    )

    buy_total = (
        (req.buy_costs.unit_purchase_price + req.buy_costs.quality_inspection_cost) * vol
        + req.buy_costs.ordering_cost
        + (req.buy_costs.carrying_cost_per_unit * vol)
    )

    difference = make_total - buy_total
    recommendation = "MAKE" if make_total < buy_total else "BUY"

    make_per_unit = make_total / vol if vol else 0
    buy_per_unit = buy_total / vol if vol else 0

    qualitative_factors = [
        "Quality control over production process",
        "Lead time and delivery reliability",
        "Supplier dependency risk",
        "Strategic core competency",
        "Capacity utilization",
        "Intellectual property protection",
    ]
    if req.quality_considerations:
        qualitative_factors.insert(0, req.quality_considerations)

    return MakeOrBuyResult(
        company_id=req.company_id,
        product_name=req.product_name,
        annual_volume=vol,
        make_total_cost=round(make_total, 2),
        buy_total_cost=round(buy_total, 2),
        difference=round(difference, 2),
        recommendation=recommendation,
        make_cost_per_unit=round(make_per_unit, 2),
        buy_cost_per_unit=round(buy_per_unit, 2),
        qualitative_factors=qualitative_factors,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
