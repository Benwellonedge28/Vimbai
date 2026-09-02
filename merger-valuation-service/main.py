"""
Merger Valuation Service
Port: 8241
M&A valuation and deal structuring
"""

import math
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Merger Valuation Service", version="1.0.0")


class MergerValuationRequest(BaseModel):
    acquirer_id: str
    target_id: str
    acquirer_equity: float
    target_equity: float
    acquirer_ebitda: float
    target_ebitda: float
    acquirer_revenue: float
    target_revenue: float
    synergies_ebitda: float
    acquirer_pe: float
    target_pe: float
    deal_premium: float
    transaction_costs: float
    integration_costs: float


class MergerValuationResponse(BaseModel):
    acquirer_id: str
    target_id: str
    valuation_methods: Dict[str, Dict[str, Any]]
    recommended_offer_price: float
    offer_premium: float
    total_deal_value: float
    synergy_value: float
    eps_impact: float
    accretive_dilutive: str
    irr_projections: Dict[str, float]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "merger-valuation", "version": "1.0.0"}


@app.post("/value", response_model=MergerValuationResponse)
async def value_merger(request: MergerValuationRequest):
    logger.info("Valuing merger", acquirer=request.acquirer_id, target=request.target_id)

    target_dcf = request.target_equity * 1.1
    target_comps = request.target_equity * (1 + request.deal_premium)
    target_asset = request.target_equity * 0.9

    combined_equity = request.acquirer_equity + request.target_equity
    combined_ebitda = request.acquirer_ebitda + request.target_ebitda + request.synergies_ebitda

    offer_price = request.target_equity * (1 + request.deal_premium)
    total_deal = offer_price + request.transaction_costs

    acquirer_new_pe = combined_equity / combined_ebitda
    pe_implied = offer_price / request.target_ebitda

    combined_eps = combined_equity / (request.acquirer_revenue + request.target_revenue)
    current_eps = request.acquirer_equity / request.acquirer_revenue
    eps_impact = combined_eps - current_eps
    accretive = "Accretive" if eps_impact > 0 else "Dilutive"

    irr_1yr = (target_comps / offer_price - 1) * 100
    irr_2yr = (target_comps / offer_price - 1) * 50
    irr_3yr = (target_comps / offer_price - 1) * 33.33

    synergy_value = request.synergies_ebitda * target_comps / request.target_ebitda

    recommendations = []
    if pe_implied < request.acquirer_pe * 1.2:
        recommendations.append("Deal appears accretive - proceed with valuation")
    if request.deal_premium > 0.4:
        recommendations.append("High premium requested - ensure synergy case supports valuation")
    if irr_1yr < 15:
        recommendations.append("Consider negotiating lower price to improve returns")
    if request.integration_costs > total_deal * 0.1:
        recommendations.append("Integration costs are significant - model full cost impact")

    return MergerValuationResponse(
        acquirer_id=request.acquirer_id,
        target_id=request.target_id,
        valuation_methods={
            "dcf": {"value": round(target_dcf, 2), "weight": 0.4},
            "comparables": {"value": round(target_comps, 2), "weight": 0.4},
            "asset": {"value": round(target_asset, 2), "weight": 0.2},
        },
        recommended_offer_price=round(offer_price, 2),
        offer_premium=round(request.deal_premium * 100, 2),
        total_deal_value=round(total_deal, 2),
        synergy_value=round(synergy_value, 2),
        eps_impact=round(eps_impact, 4),
        accretive_dilutive=accretive,
        irr_projections={"1yr": round(irr_1yr, 2), "2yr": round(irr_2yr, 2), "3yr": round(irr_3yr, 2)},
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8241)
