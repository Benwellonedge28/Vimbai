"""
Swap Valuation Service
Port: 8249
Interest rate and currency swap valuation
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
import math

logger = structlog.get_logger()
app = FastAPI(title="Swap Valuation Service", version="1.0.0")

class Swap(BaseModel):
    swap_id: str
    swap_type: str
    notional: float
    fixed_rate: float
    floating_rate: float
    tenor_years: int
    payment_frequency: str

class SwapValuationRequest(BaseModel):
    company_id: str
    swaps: List[Swap]
    current_yield_curve: Dict[str, float]
    ois_rate: float

class SwapValuationResponse(BaseModel):
    company_id: str
    swap_valuations: List[Dict[str, Any]]
    portfolio_summary: Dict[str, Any]
    counterparty_exposure: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "swap-valuation", "version": "1.0.0"}

@app.post("/value", response_model=SwapValuationResponse)
async def value_swaps(request: SwapValuationRequest):
    logger.info("Valuing swaps", company=request.company_id)

    swap_valuations = []
    total_mtm = 0
    
    for swap in request.swaps:
        payments_per_year = 2 if swap.payment_frequency == "semi-annual" else 4 if swap.payment_frequency == "quarterly" else 1
        
        discount_factor = sum(1 / ((1 + request.ois_rate) ** (i / payments_per_year)) for i in range(1, swap.tenor_years * payments_per_year + 1))
        fixed_pv = swap.notional * swap.fixed_rate / payments_per_year * discount_factor
        
        avg_floating = swap.floating_rate
        floating_pv = swap.notional * avg_floating / payments_per_year * discount_factor
        
        mtm = fixed_pv - floating_pv
        swap_valuations.append({
            "swap_id": swap.swap_id,
            "type": swap.swap_type,
            "notional": swap.notional,
            "fixed_rate": swap.fixed_rate,
            "floating_rate": swap.floating_rate,
            "fixed_leg_pv": round(fixed_pv, 2),
            "floating_leg_pv": round(floating_pv, 2),
            "market_value": round(mtm, 2),
            "market_value_pct": round(mtm / swap.notional * 100, 4),
            "receive_fixed": mtm > 0
        })
        total_mtm += mtm
    
    receive_fixed = sum(s["market_value"] for s in swap_valuations if s["receive_fixed"])
    pay_fixed = sum(s["market_value"] for s in swap_valuations if not s["receive_fixed"])
    
    portfolio_summary = {
        "total_mtm": round(total_mtm, 2),
        "receive_fixed_pv": round(receive_fixed, 2),
        "pay_fixed_pv": round(pay_fixed, 2),
        "swap_count": len(request.swaps)
    }
    
    counterparty_exposure = {
        "net_exposure": round(max(receive_fixed - pay_fixed, 0), 2),
        "gross_exposure": round(receive_fixed + abs(pay_fixed), 2),
        "largest_exposure": max(s["market_value"] for s in swap_valuations) if swap_valuations else 0
    }
    
    recommendations = []
    if total_mtm < 0 and abs(total_mtm) > sum(s["notional"] for s in request.swaps) * 0.1:
        recommendations.append("Significant negative MTM - monitor counterparty risk")
    if counterparty_exposure["largest_exposure"] > sum(s["notional"] for s in request.swaps) * 0.3:
        recommendations.append("Concentrated exposure to single counterparty - consider diversification")

    return SwapValuationResponse(
        company_id=request.company_id,
        swap_valuations=swap_valuations,
        portfolio_summary=portfolio_summary,
        counterparty_exposure=counterparty_exposure,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8249)
