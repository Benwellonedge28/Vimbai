"""
Deal Structuring Service
Port: 8246
M&A deal structure optimization
"""

from datetime import datetime
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Deal Structuring Service", version="1.0.0")


class DealComponent(BaseModel):
    component_type: str
    value: float
    percentage: float
    conditions: List[str]


class DealStructuringRequest(BaseModel):
    deal_id: str
    target_id: str
    total_value: float
    seller_preference: str
    tax_efficiency_priority: bool
    earnout_required: bool
    escrow_percentage: float
    locked_box_date: str


class DealStructuringResponse(BaseModel):
    deal_id: str
    deal_date: str
    structure_options: List[Dict[str, Any]]
    recommended_structure: Dict[str, Any]
    tax_considerations: Dict[str, Any]
    risk_allocation: Dict[str, Any]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "deal-structuring", "version": "1.0.0"}


@app.post("/structure", response_model=DealStructuringResponse)
async def structure_deal(request: DealStructuringRequest):
    logger.info("Structuring deal", deal=request.deal_id)

    cash_deal = {
        "structure_type": "All Cash",
        "upfront_payment": request.total_value,
        "deferred_payment": 0,
        "earnout": 0,
        "escrow": request.total_value * 0.1,
        "tax_efficiency": 0.7,
        "speed_close": 0.9,
        "risk_transfer": 0.8,
    }

    share_deal = {
        "structure_type": "Share Exchange",
        "upfront_payment": request.total_value * 0.8,
        "deferred_payment": 0,
        "earnout": request.total_value * 0.2,
        "escrow": request.total_value * 0.05,
        "tax_efficiency": 0.9,
        "speed_close": 0.5,
        "risk_transfer": 0.6,
    }

    hybrid = {
        "structure_type": "Hybrid (Cash + Stock)",
        "upfront_payment": request.total_value * 0.6,
        "deferred_payment": request.total_value * 0.2,
        "earnout": request.total_value * 0.2,
        "escrow": request.total_value * 0.08,
        "tax_efficiency": 0.85,
        "speed_close": 0.7,
        "risk_transfer": 0.7,
    }

    if request.seller_preference == "cash":
        recommended = cash_deal
    elif request.seller_preference == "stock":
        recommended = share_deal
    else:
        recommended = hybrid

    tax_considerations = {
        "asset_deal": {"capital_gains_exposure": "High", "step_up_benefit": True, "liability_assumption": True},
        "share_deal": {"capital_gains_exposure": "Medium", "step_up_benefit": False, "liability_assumption": False},
        "recommended": (
            "Hybrid" if request.tax_efficiency_priority else "Share" if request.seller_preference == "stock" else "Cash"
        ),
    }

    risk_allocation = {
        "completion_risk": {"buyer": 0.7, "seller": 0.3},
        "earnout_risk": {"buyer": 0.5, "seller": 0.5},
        "warranty_risk": {"buyer": 0.3, "seller": 0.7},
    }

    recommendations = []
    if request.tax_efficiency_priority:
        recommendations.append("Consider share exchange to optimize seller tax position")
    if request.earnout_required:
        recommendations.append("Structure earnout with clear KPIs and caps")
    if request.escrow_percentage < 0.1:
        recommendations.append("Consider higher escrow for warranty protection")

    return DealStructuringResponse(
        deal_id=request.deal_id,
        deal_date=datetime.now().isoformat(),
        structure_options=[cash_deal, share_deal, hybrid],
        recommended_structure=recommended,
        tax_considerations=tax_considerations,
        risk_allocation=risk_allocation,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8246)
