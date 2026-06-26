"""
Commodity Hedging Service
Port: 8253
Commodity price risk management
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
import math

logger = structlog.get_logger()
app = FastAPI(title="Commodity Hedging Service", version="1.0.0")

class CommodityExposure(BaseModel):
    commodity: str
    exposure_type: str
    volume: float
    unit: str
    current_price: float
    budget_price: float
    volatility: float

class CommodityHedgingRequest(BaseModel):
    company_id: str
    exposures: List[CommodityExposure]
    hedge_ratio: float
    budget_rate: Dict[str, float]

class CommodityHedgingResponse(BaseModel):
    company_id: str
    exposure_analysis: List[Dict[str, Any]]
    hedge_plan: List[Dict[str, Any]]
    budget_impact: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "commodity-hedging", "version": "1.0.0"}

@app.post("/analyze", response_model=CommodityHedgingResponse)
async def analyze_commodity_hedge(request: CommodityHedgingRequest):
    logger.info("Analyzing commodity hedging", company=request.company_id)

    exposure_analysis = []
    hedge_plan = []
    total_exposure_value = 0
    total_budget_value = 0
    
    for exp in request.exposures:
        exposure_value = exp.volume * exp.current_price
        budget_value = exp.volume * request.budget_rate.get(exp.commodity, exp.budget_price)
        
        price_upside = max(exp.current_price - exp.budget_price, 0) * exp.volume
        price_downside = max(exp.budget_price - exp.current_price, 0) * exp.volume
        
        exposure_analysis.append({
            "commodity": exp.commodity,
            "exposure_type": exp.exposure_type,
            "volume": exp.volume,
            "unit": exp.unit,
            "current_price": exp.current_price,
            "exposure_value": round(exposure_value, 2),
            "budget_price": exp.budget_price,
            "budget_value": round(budget_value, 2),
            "volatility": round(exp.volatility * 100, 2),
            "upside_risk": round(price_upside, 2),
            "downside_risk": round(price_downside, 2)
        })
        
        hedged_volume = exp.volume * request.hedge_ratio
        hedge_cost = hedged_volume * exp.current_price * exp.volatility * 0.15
        
        hedge_plan.append({
            "commodity": exp.commodity,
            "hedge_type": "Futures" if exp.volatility > 0.25 else "Options",
            "hedge_volume": round(hedged_volume, 2),
            "hedge_percentage": round(request.hedge_ratio * 100, 2),
            "estimated_cost": round(hedge_cost, 2),
            "protection_level": round(exp.current_price * (1 - exp.volatility), 2)
        })
        
        total_exposure_value += exposure_value
        total_budget_value += budget_value
    
    variance = total_exposure_value - total_budget_value
    variance_pct = variance / total_budget_value if total_budget_value else 0
    
    budget_impact = {
        "total_exposure": round(total_exposure_value, 2),
        "budget_value": round(total_budget_value, 2),
        "variance": round(variance, 2),
        "variance_pct": round(variance_pct * 100, 2),
        "status": "Over Budget" if variance > 0 else "Under Budget"
    }
    
    recommendations = []
    if variance > total_budget_value * 0.1:
        recommendations.append("Commodity costs significantly over budget - increase hedges")
    if request.hedge_ratio < 0.5:
        recommendations.append("Low hedge ratio - consider increasing coverage")
    if any(e["volatility"] > 30 for e in exposure_analysis):
        recommendations.append("High volatility commodities need options-based hedging")

    return CommodityHedgingResponse(
        company_id=request.company_id,
        exposure_analysis=exposure_analysis,
        hedge_plan=hedge_plan,
        budget_impact=budget_impact,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8253)
