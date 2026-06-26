"""
Interest Rate Hedging Service
Port: 8252
Interest rate risk management
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
import math

logger = structlog.get_logger()
app = FastAPI(title="Interest Rate Hedging Service", version="1.0.0")

class DebtExposure(BaseModel):
    debt_id: str
    principal: float
    current_rate: float
    rate_type: str
    maturity_years: float
    reference_rate: str

class InterestRateHedgingRequest(BaseModel):
    company_id: str
    exposures: List[DebtExposure]
    total_debt: float
    variable_debt: float
    rate_shock_scenarios: List[float]
    yield_curve_slope: float

class InterestRateHedgingResponse(BaseModel):
    company_id: str
    exposure_summary: Dict[str, Any]
    sensitivity_analysis: List[Dict[str, Any]]
    hedge_recommendations: List[Dict[str, Any]]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "interest-rate-hedging", "version": "1.0.0"}

@app.post("/analyze", response_model=InterestRateHedgingResponse)
async def analyze_interest_rate_hedge(request: InterestRateHedgingRequest):
    logger.info("Analyzing interest rate hedging", company=request.company_id)

    variable_debt_pct = request.variable_debt / request.total_debt if request.total_debt else 0
    fixed_debt_pct = 1 - variable_debt_pct
    
    exposure_summary = {
        "total_debt": request.total_debt,
        "variable_debt": request.variable_debt,
        "fixed_debt": request.total_debt - request.variable_debt,
        "variable_debt_pct": round(variable_debt_pct * 100, 2),
        "fixed_debt_pct": round(fixed_debt_pct * 100, 2)
    }
    
    sensitivity_analysis = []
    for shock in request.rate_shock_scenarios:
        current_interest = request.variable_debt * 0.05
        shocked_interest = request.variable_debt * (0.05 + shock)
        additional_cost = shocked_interest - current_interest
        
        sensitivity_analysis.append({
            "rate_shock_bp": round(shock * 10000, 0),
            "additional_annual_cost": round(additional_cost, 2),
            "monthly_impact": round(additional_cost / 12, 2),
            "ebitda_impact_pct": round(additional_cost / request.total_debt * 100, 2)
        })
    
    weighted_avg_maturity = sum(e.principal * e.maturity_years for e in request.exposures) / sum(e.principal for e in request.exposures) if request.exposures else 0
    
    hedge_recommendations = []
    if variable_debt_pct > 0.6:
        hedge_recommendations.append({
            "action": "FIX",
            "amount": round(request.variable_debt * 0.3, 2),
            "instrument": "Interest Rate Swap",
            "target_rate": round(0.05 + request.yield_curve_slope * 2, 4),
            "tenor": "3 years"
        })
    if weighted_avg_maturity < 2:
        hedge_recommendations.append({
            "action": "EXTEND",
            "amount": round(request.total_debt * 0.2, 2),
            "instrument": "Fixed Rate Bond",
            "target_rate": round(0.05 + request.yield_curve_slope, 4),
            "tenor": "5 years"
        })
    
    recommendations = []
    if variable_debt_pct > 0.7:
        recommendations.append("High variable rate exposure - consider swapping to fixed")
    if weighted_avg_maturity < 1.5:
        recommendations.append("Short debt maturity - extend to reduce refinancing risk")

    return InterestRateHedgingResponse(
        company_id=request.company_id,
        exposure_summary=exposure_summary,
        sensitivity_analysis=sensitivity_analysis,
        hedge_recommendations=hedge_recommendations,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8252)
