"""
Fund Management Service
Port: 8266
Fund performance and analysis
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
import math

logger = structlog.get_logger()
app = FastAPI(title="Fund Management Service", version="1.0.0")

class FundHolding(BaseModel):
    holding_id: str
    security_name: str
    shares: float
    price: float
    weight: float

class FundManagementRequest(BaseModel):
    fund_id: str
    fund_name: str
    holdings: List[FundHolding]
    benchmark_return: float
    risk_free_rate: float

class FundManagementResponse(BaseModel):
    fund_id: str
    fund_name: str
    fund_metrics: Dict[str, Any]
    top_holdings: List[Dict[str, Any]]
    performance_metrics: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "fund-management", "version": "1.0.0"}

@app.post("/analyze", response_model=FundManagementResponse)
async def analyze_fund(request: FundManagementRequest):
    logger.info("Analyzing fund", fund=request.fund_id)

    total_nav = sum(h.shares * h.price for h in request.holdings)
    
    holdings_analysis = []
    for h in request.holdings:
        value = h.shares * h.price
        holdings_analysis.append({
            "holding_id": h.holding_id,
            "security_name": h.security_name,
            "value": round(value, 2),
            "weight": round(value / total_nav * 100, 2) if total_nav else 0,
            "shares": h.shares,
            "price": h.price
        })
    
    holdings_analysis.sort(key=lambda x: x["value"], reverse=True)
    top_holdings = holdings_analysis[:10]
    
    top10_weight = sum(h["weight"] for h in top_holdings)
    
    fund_metrics = {
        "total_nav": round(total_nav, 2),
        "holding_count": len(request.holdings),
        "top10_weight": round(top10_weight, 2)
    }
    
    alpha = 0.05 - request.benchmark_return
    tracking_error = 0.02
    info_ratio = alpha / tracking_error if tracking_error else 0
    
    performance_metrics = {
        "fund_return": 0.08,
        "benchmark_return": request.benchmark_return,
        "alpha": round(alpha * 100, 2),
        "tracking_error": round(tracking_error * 100, 2),
        "information_ratio": round(info_ratio, 4)
    }
    
    recommendations = []
    if top10_weight > 50:
        recommendations.append("High concentration in top 10 holdings - consider diversifying")
    if alpha < 0:
        recommendations.append("Negative alpha - review investment strategy")
    if len(request.holdings) < 20:
        recommendations.append("Low diversification - add more holdings")

    return FundManagementResponse(
        fund_id=request.fund_id,
        fund_name=request.fund_name,
        fund_metrics=fund_metrics,
        top_holdings=top_holdings,
        performance_metrics=performance_metrics,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8266)
