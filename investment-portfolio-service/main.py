"""
Investment Portfolio Service
Port: 8265
Investment portfolio management
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
import math

logger = structlog.get_logger()
app = FastAPI(title="Investment Portfolio Service", version="1.0.0")

class Investment(BaseModel):
    asset_id: str
    asset_type: str
    current_value: float
    cost_basis: float
    expected_return: float
    volatility: float

class InvestmentPortfolioRequest(BaseModel):
    company_id: str
    investments: List[Investment]
    risk_free_rate: float
    target_return: float

class InvestmentPortfolioResponse(BaseModel):
    company_id: str
    portfolio_summary: Dict[str, Any]
    allocation_analysis: List[Dict[str, Any]]
    risk_metrics: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "investment-portfolio", "version": "1.0.0"}

@app.post("/analyze", response_model=InvestmentPortfolioResponse)
async def analyze_investment_portfolio(request: InvestmentPortfolioRequest):
    logger.info("Analyzing investment portfolio", company=request.company_id)

    total_value = sum(inv.current_value for inv in request.investments)
    total_cost = sum(inv.cost_basis for inv in request.investments)
    total_gain = total_value - total_cost
    total_return_pct = (total_gain / total_cost * 100) if total_cost else 0
    
    by_type = {}
    for inv in request.investments:
        if inv.asset_type not in by_type:
            by_type[inv.asset_type] = {"value": 0, "cost": 0}
        by_type[inv.asset_type]["value"] += inv.current_value
        by_type[inv.asset_type]["cost"] += inv.cost_basis
    
    allocation_analysis = []
    for atype, data in by_type.items():
        allocation_analysis.append({
            "asset_type": atype,
            "value": round(data["value"], 2),
            "cost": round(data["cost"], 2),
            "allocation_pct": round(data["value"] / total_value * 100, 2) if total_value else 0,
            "gain": round(data["value"] - data["cost"], 2)
        })
    
    port_return = sum(inv.current_value * inv.expected_return for inv in request.investments) / total_value if total_value else 0
    port_vol = math.sqrt(sum((inv.current_value / total_value) ** 2 * inv.volatility ** 2 for inv in request.investments)) if total_value else 0
    sharpe = (port_return - request.risk_free_rate) / port_vol if port_vol else 0
    
    portfolio_summary = {
        "total_value": round(total_value, 2),
        "total_cost": round(total_cost, 2),
        "total_gain": round(total_gain, 2),
        "total_return_pct": round(total_return_pct, 2),
        "asset_count": len(request.investments)
    }
    
    risk_metrics = {
        "portfolio_return": round(port_return * 100, 2),
        "portfolio_volatility": round(port_vol * 100, 2),
        "sharpe_ratio": round(sharpe, 4),
        "risk_free_rate": round(request.risk_free_rate * 100, 2)
    }
    
    recommendations = []
    if sharpe < 0.5:
        recommendations.append("Low Sharpe ratio - consider rebalancing portfolio")
    if port_vol > 0.2:
        recommendations.append("High volatility - consider reducing risk")
    if total_return_pct < 0:
        recommendations.append("Portfolio in loss - review asset allocation")

    return InvestmentPortfolioResponse(
        company_id=request.company_id,
        portfolio_summary=portfolio_summary,
        allocation_analysis=allocation_analysis,
        risk_metrics=risk_metrics,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8265)
