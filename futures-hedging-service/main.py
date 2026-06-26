"""
Futures Hedging Service
Port: 8250
Futures hedging strategy optimization
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
import math

logger = structlog.get_logger()
app = FastAPI(title="Futures Hedging Service", version="1.0.0")

class FuturePosition(BaseModel):
    future_id: str
    contract_month: str
    position_size: float
    entry_price: float
    current_price: float
    contract_size: float

class FuturesHedgingRequest(BaseModel):
    company_id: str
    positions: List[FuturePosition]
    spot_exposure: float
    target_hedge_ratio: float
    basis_risk: float

class FuturesHedgingResponse(BaseModel):
    company_id: str
    position_analysis: List[Dict[str, Any]]
    hedge_effectiveness: Dict[str, Any]
    optimal_hedge_ratio: float
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "futures-hedging", "version": "1.0.0"}

@app.post("/analyze", response_model=FuturesHedgingResponse)
async def analyze_futures_hedge(request: FuturesHedgingRequest):
    logger.info("Analyzing futures hedging", company=request.company_id)

    position_analysis = []
    total_unrealized_pnl = 0
    total_exposure = 0
    
    for pos in request.positions:
        contract_value = pos.position_size * pos.contract_size
        unrealized_pnl = (pos.current_price - pos.entry_price) * pos.position_size * pos.contract_size
        daily_var = pos.contract_size * pos.position_size * request.basis_risk
        
        position_analysis.append({
            "future_id": pos.future_id,
            "contract_month": pos.contract_month,
            "position_size": pos.position_size,
            "entry_price": pos.entry_price,
            "current_price": pos.current_price,
            "contract_value": round(contract_value, 2),
            "unrealized_pnl": round(unrealized_pnl, 2),
            "pnl_percentage": round(unrealized_pnl / contract_value * 100, 4),
            "daily_var": round(daily_var, 2)
        })
        
        total_unrealized_pnl += unrealized_pnl
        total_exposure += contract_value
    
    current_hedge_ratio = total_exposure / request.spot_exposure if request.spot_exposure else 0
    hedge_effectiveness = 1 - request.basis_risk
    
    optimal_hedge_ratio = min(request.target_hedge_ratio, 0.95)
    shortfall = request.spot_exposure * (optimal_hedge_ratio - current_hedge_ratio)
    
    recommendations = []
    if abs(current_hedge_ratio - request.target_hedge_ratio) > 0.1:
        recommendations.append(f"Hedge ratio deviates from target by {abs(current_hedge_ratio - request.target_hedge_ratio):.2%}")
    if request.basis_risk > 0.02:
        recommendations.append("Basis risk is elevated - monitor spread closely")
    if shortfall > 0:
        recommendations.append(f"Add {shortfall:.0f} notional to reach optimal hedge ratio")

    return FuturesHedgingResponse(
        company_id=request.company_id,
        position_analysis=position_analysis,
        hedge_effectiveness={
            "current_ratio": round(current_hedge_ratio, 4),
            "target_ratio": request.target_hedge_ratio,
            "effectiveness": round(hedge_effectiveness * 100, 2),
            "basis_risk": round(request.basis_risk * 100, 2)
        },
        optimal_hedge_ratio=round(optimal_hedge_ratio, 4),
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8250)
