"""
Credit Derivatives Service
Port: 8254
Credit risk hedging instruments
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
import math

logger = structlog.get_logger()
app = FastAPI(title="Credit Derivatives Service", version="1.0.0")

class CreditPosition(BaseModel):
    position_id: str
    instrument_type: str
    reference_entity: str
    notional: float
    spread: float
    rating: str
    maturity_years: float

class CreditDerivativesRequest(BaseModel):
    company_id: str
    positions: List[CreditPosition]
    risk_free_rate: float
    recovery_rate: float
    credit_spreads: Dict[str, float]

class CreditDerivativesResponse(BaseModel):
    company_id: str
    position_valuation: List[Dict[str, Any]]
    portfolio_exposure: Dict[str, Any]
    cva_calculation: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "credit-derivatives", "version": "1.0.0"}

@app.post("/analyze", response_model=CreditDerivativesResponse)
async def analyze_credit_derivatives(request: CreditDerivativesRequest):
    logger.info("Analyzing credit derivatives", company=request.company_id)

    position_valuation = []
    total_exposure = 0
    total_mtm = 0
    
    for pos in request.positions:
        market_spread = request.credit_spreads.get(pos.reference_entity, pos.spread)
        spread_diff = pos.spread - market_spread
        
        pv_factor = math.exp(-request.risk_free_rate * pos.maturity_years)
        expected_loss = pos.notional * (1 - request.recovery_rate) * 0.01
        
        cds_value = pos.notional * spread_diff / 10000 * pos.maturity_years * pv_factor
        
        if pos.instrument_type == "CDS":
            premium_leg = pos.notional * pos.spread / 10000 * pos.maturity_years * pv_factor
            protection_leg = expected_loss * pv_factor
            mtm = premium_leg - protection_leg
        else:
            mtm = pos.notional * spread_diff / 10000 * pv_factor
        
        position_valuation.append({
            "position_id": pos.position_id,
            "instrument": pos.instrument_type,
            "reference_entity": pos.reference_entity,
            "notional": pos.notional,
            "current_spread": pos.spread,
            "market_spread": round(market_spread, 2),
            "spread_diff": round(spread_diff, 2),
            "mtm": round(mtm, 2),
            "mtm_pct": round(mtm / pos.notional * 100, 4),
            "rating": pos.rating
        })
        
        total_exposure += pos.notional
        total_mtm += mtm
    
    total_cva = sum(pos.notional * (1 - request.recovery_rate) * 0.01 * math.exp(-request.risk_free_rate * pos.maturity_years) for pos in request.positions)
    
    portfolio_exposure = {
        "total_notional": total_exposure,
        "net_mtm": round(total_mtm, 2),
        "exposure_count": len(request.positions),
        "avg_maturity": round(sum(p.maturity_years for p in request.positions) / len(request.positions), 2) if request.positions else 0
    }
    
    cva_calculation = {
        "total_cva": round(total_cva, 2),
        "cva_pct_notional": round(total_cva / total_exposure * 100, 4) if total_exposure else 0,
        "recovery_assumption": request.recovery_rate
    }
    
    recommendations = []
    if total_mtm < -total_exposure * 0.05:
        recommendations.append("Significant negative MTM - review counterparty credit risk")
    if portfolio_exposure["avg_maturity"] > 5:
        recommendations.append("Long-dated credit exposure - monitor spread volatility")

    return CreditDerivativesResponse(
        company_id=request.company_id,
        position_valuation=position_valuation,
        portfolio_exposure=portfolio_exposure,
        cva_calculation=cva_calculation,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8254)
