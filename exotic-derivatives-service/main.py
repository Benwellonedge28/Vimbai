"""
Exotic Derivatives Service
Port: 8255
Exotic options and structured products valuation
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
import math

logger = structlog.get_logger()
app = FastAPI(title="Exotic Derivatives Service", version="1.0.0")

class ExoticOption(BaseModel):
    option_id: str
    option_type: str
    underlying: str
    strike: float
    spot: float
    barrier: float
    notional: float
    maturity: float
    volatility: float
    rate: float

class ExoticDerivativesRequest(BaseModel):
    company_id: str
    options: List[ExoticOption]

class ExoticDerivativesResponse(BaseModel):
    company_id: str
    valuation_results: List[Dict[str, Any]]
    risk_metrics: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "exotic-derivatives", "version": "1.0.0"}

@app.post("/value", response_model=ExoticDerivativesResponse)
async def value_exotic_derivatives(request: ExoticDerivativesRequest):
    logger.info("Valuing exotic derivatives", company=request.company_id)

    valuation_results = []
    total_value = 0
    total_vega = 0
    
    for opt in request.options:
        d1 = (math.log(opt.spot / opt.strike) + (opt.rate + opt.volatility ** 2 / 2) * opt.maturity) / (opt.volatility * math.sqrt(opt.maturity))
        
        vanilla_price = opt.spot * math.exp(-opt.rate * opt.maturity) * 0.5 * (1 + math erf(d1 / math.sqrt(2))) - opt.strike * math.exp(-opt.rate * opt.maturity) * 0.5 * (1 + math erf((d1 - opt.volatility * math.sqrt(opt.maturity)) / math.sqrt(2)))
        
        if opt.option_type in ["knockout", "knock-in"]:
            barrier_dist = abs(math.log(opt.barrier / opt.spot))
            barrier_prob = 1 - math.exp(-2 * barrier_dist * barrier_dist / (opt.volatility * opt.volatility * opt.maturity))
            
            if opt.option_type == "knockout":
                exotic_price = vanilla_price * (1 - barrier_prob * 0.7)
            else:
                exotic_price = vanilla_price * (1 + barrier_prob * 0.3)
        elif opt.option_type == "digital":
            exotic_price = opt.notional * math.exp(-opt.rate * opt.maturity) * 0.5 * (1 + math erf(-d1 / math.sqrt(2)))
        elif opt.option_type == "range_accrual":
            exotic_price = vanilla_price * 0.6
        else:
            exotic_price = vanilla_price * 0.9
        
        delta = 0.5 * (1 + math erf(d1 / math.sqrt(2)))
        gamma = math.exp(-d1 * d1 / 2) / (opt.spot * opt.volatility * math.sqrt(2 * math.pi * opt.maturity))
        vega = opt.spot * math.sqrt(opt.maturity) * math.exp(-d1 * d1 / 2) / (opt.volatility * math.sqrt(2 * math.pi)) / 100
        
        valuation_results.append({
            "option_id": opt.option_id,
            "type": opt.option_type,
            "underlying": opt.underlying,
            "strike": opt.strike,
            "spot": opt.spot,
            "barrier": opt.barrier,
            "vanilla_price": round(vanilla_price, 4),
            "exotic_price": round(exotic_price, 4),
            "premium": round(exotic_price - vanilla_price, 4),
            "premium_pct": round((exotic_price - vanilla_price) / vanilla_price * 100, 2) if vanilla_price else 0,
            "delta": round(delta, 4),
            "gamma": round(gamma, 6),
            "vega": round(vega, 4),
            "in_the_money": opt.spot > opt.strike if "call" in opt.option_type.lower() else opt.spot < opt.strike
        })
        
        total_value += exotic_price * opt.notional
        total_vega += vega * opt.notional
    
    risk_metrics = {
        "total_portfolio_value": round(total_value, 2),
        "total_vega_exposure": round(total_vega, 4),
        "option_count": len(request.options),
        "avg_premium": round(sum(v["premium_pct"] for v in valuation_results) / len(valuation_results), 2) if valuation_results else 0
    }
    
    recommendations = []
    if abs(total_vega) > 10000:
        recommendations.append("High vega exposure - monitor volatility sensitivity")
    if any(v["premium_pct"] > 50 for v in valuation_results):
        recommendations.append("Exotic premium is high - consider simpler structures")

    return ExoticDerivativesResponse(
        company_id=request.company_id,
        valuation_results=valuation_results,
        risk_metrics=risk_metrics,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8255)
