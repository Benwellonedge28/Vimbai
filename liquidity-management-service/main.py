"""
Liquidity Management Service
Port: 8239
Liquidity risk assessment and management
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
import math

logger = structlog.get_logger()
app = FastAPI(title="Liquidity Management Service", version="1.0.0")

class LiquidityPosition(BaseModel):
    cash: float
    marketable_securities: float
    credit_facilities_available: float
    total_liquid_assets: float

class LiquidityRequest(BaseModel):
    company_id: str
    cash: float
    marketable_securities: float
    credit_facilities_available: float
    credit_facilities_total: float
    current_assets: float
    current_liabilities: float
    monthly_fixed_costs: float
    monthly_variable_costs: float
    monthly_min_revenue: float

class LiquidityResponse(BaseModel):
    company_id: str
    position: LiquidityPosition
    liquidity_ratios: Dict[str, float]
    coverage_days: float
    stress_test: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "liquidity-management", "version": "1.0.0"}

@app.post("/analyze", response_model=LiquidityResponse)
async def analyze_liquidity(request: LiquidityRequest):
    logger.info("Analyzing liquidity", company=request.company_id)

    total_liquid = request.cash + request.marketable_securities + request.credit_facilities_available
    current_ratio = request.current_assets / request.current_liabilities if request.current_liabilities else 0
    quick_ratio = (request.cash + request.marketable_securities) / request.current_liabilities if request.current_liabilities else 0
    cash_ratio = request.cash / request.current_liabilities if request.current_liabilities else 0
    
    utilization = (request.credit_facilities_total - request.credit_facilities_available) / request.credit_facilities_total if request.credit_facilities_total else 0
    
    monthly_costs = request.monthly_fixed_costs + request.monthly_variable_costs
    coverage_days = (total_liquid / monthly_costs * 30) if monthly_costs else 0

    position = LiquidityPosition(
        cash=request.cash,
        marketable_securities=request.marketable_securities,
        credit_facilities_available=request.credit_facilities_available,
        total_liquid_assets=total_liquid
    )

    stress_scenarios = {
        "revenue_drop_30": {
            "monthly_impact": monthly_costs * 0.3,
            "runway_days": (total_liquid / (monthly_costs * 0.3)) * 30 if monthly_costs else 0
        },
        "revenue_drop_50": {
            "monthly_impact": monthly_costs * 0.5,
            "runway_days": (total_liquid / (monthly_costs * 0.5)) * 30 if monthly_costs else 0
        },
        "payment_delay_60": {
            "monthly_impact": request.monthly_min_revenue * 0.6,
            "runway_days": (total_liquid / (request.monthly_min_revenue * 0.6)) * 30 if request.monthly_min_revenue else 0
        }
    }

    recommendations = []
    if coverage_days < 30:
        recommendations.append("CRITICAL: Less than 30 days liquidity coverage - immediate action required")
    if coverage_days < 60:
        recommendations.append("WARNING: Coverage below 60 days - consider drawing on credit facilities")
    if utilization > 0.8:
        recommendations.append("Credit facility utilization above 80% - preserve capacity")
    if quick_ratio < 1.0:
        recommendations.append("Quick ratio below 1.0 - focus on converting receivables to cash")

    return LiquidityResponse(
        company_id=request.company_id,
        position=position,
        liquidity_ratios={
            "current_ratio": round(current_ratio, 4),
            "quick_ratio": round(quick_ratio, 4),
            "cash_ratio": round(cash_ratio, 4),
            "credit_utilization": round(utilization, 4)
        },
        coverage_days=round(coverage_days, 2),
        stress_test=stress_scenarios,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8239)
