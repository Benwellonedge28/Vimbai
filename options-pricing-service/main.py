"""
Options Pricing Service
Port: 8248
Options valuation and Greeks calculation
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
import math
from scipy.stats import norm

logger = structlog.get_logger()
app = FastAPI(title="Options Pricing Service", version="1.0.0")

class Option(BaseModel):
    option_type: str
    strike_price: float
    spot_price: float
    time_to_expiry: float
    volatility: float
    risk_free_rate: float
    notional: float

class OptionsPricingRequest(BaseModel):
    company_id: str
    options: List[Option]
    current_spot: float

class OptionsPricingResponse(BaseModel):
    company_id: str
    option_valuations: List[Dict[str, Any]]
    portfolio_summary: Dict[str, Any]
    greeks_summary: Dict[str, float]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "options-pricing", "version": "1.0.0"}

def black_scholes_call(S, K, T, r, sigma):
    d1 = (math.log(S/K) + (r + sigma**2/2) * T) / (sigma * math.sqrt(T))
    d2 = d1 - sigma * math.sqrt(T)
    return S * norm.cdf(d1) - K * math.exp(-r * T) * norm.cdf(d2)

def black_scholes_put(S, K, T, r, sigma):
    d1 = (math.log(S/K) + (r + sigma**2/2) * T) / (sigma * math.sqrt(T))
    d2 = d1 - sigma * math.sqrt(T)
    return K * math.exp(-r * T) * norm.cdf(-d2) - S * norm.cdf(-d1)

@app.post("/price", response_model=OptionsPricingResponse)
async def price_options(request: OptionsPricingRequest):
    logger.info("Pricing options", company=request.company_id)

    option_valuations = []
    total_premium = 0
    total_delta = 0
    total_gamma = 0
    total_theta = 0
    total_vega = 0
    
    for opt in request.options:
        S, K, T, r, sigma = opt.spot_price, opt.strike_price, opt.time_to_expiry, opt.risk_free_rate, opt.volatility
        
        if opt.option_type == "call":
            price = black_scholes_call(S, K, T, r, sigma)
        else:
            price = black_scholes_put(S, K, T, r, sigma)
        
        d1 = (math.log(S/K) + (r + sigma**2/2) * T) / (sigma * math.sqrt(T))
        delta = norm.cdf(d1) if opt.option_type == "call" else norm.cdf(d1) - 1
        gamma = norm.pdf(d1) / (S * sigma * math.sqrt(T))
        theta = (-S * norm.pdf(d1) * sigma / (2 * math.sqrt(T)) - r * K * math.exp(-r * T) * norm.cdf(d1 if opt.option_type == "call" else -d1)) / 365
        vega = S * math.sqrt(T) * norm.pdf(d1) / 100
        
        option_valuations.append({
            "type": opt.option_type,
            "strike": K,
            "spot": S,
            "premium": round(price, 4),
            "premium_pct": round(price / S * 100, 4),
            "moneyness": "ITM" if (opt.option_type == "call" and S > K) or (opt.option_type == "put" and S < K) else "ATM" if S == K else "OTM",
            "delta": round(delta, 4),
            "gamma": round(gamma, 6),
            "theta": round(theta, 4),
            "vega": round(vega, 4),
            "intrinsic_value": max(S - K, 0) if opt.option_type == "call" else max(K - S, 0),
            "time_value": price - (max(S - K, 0) if opt.option_type == "call" else max(K - S, 0))
        })
        
        total_premium += price * opt.notional
        total_delta += delta * opt.notional
        total_gamma += gamma * opt.notional
        total_theta += theta * opt.notional
        total_vega += vega * opt.notional
    
    portfolio_summary = {
        "total_premium": round(total_premium, 2),
        "option_count": len(request.options),
        "avg_premium_pct": round(total_premium / (sum(o.spot_price * o.notional for o in request.options)) * 100, 4) if request.options else 0
    }
    
    greeks_summary = {
        "delta": round(total_delta, 4),
        "gamma": round(total_gamma, 6),
        "theta": round(total_theta, 4),
        "vega": round(total_vega, 4)
    }
    
    recommendations = []
    if abs(total_delta) > sum(o.notional for o in request.options) * 0.8:
        recommendations.append("High delta exposure - consider delta hedging")
    if total_gamma > 0.1:
        recommendations.append("Gamma risk elevated - monitor position closely")
    if greeks_summary["theta"] < -10000:
        recommendations.append("Significant time decay - assess holding period")

    return OptionsPricingResponse(
        company_id=request.company_id,
        option_valuations=option_valuations,
        portfolio_summary=portfolio_summary,
        greeks_summary=greeks_summary,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8248)
