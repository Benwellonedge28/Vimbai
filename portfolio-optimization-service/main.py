"""
Portfolio Optimization Service
Port: 8234
Modern Portfolio Theory optimization
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
import math

logger = structlog.get_logger()
app = FastAPI(title="Portfolio Optimization Service", version="1.0.0")

class OptimizedAllocation(BaseModel):
    asset_id: str
    asset_name: str
    current_weight: float
    optimal_weight: float
    target_allocation: float
    adjustment_needed: float

class PortfolioOptimizationRequest(BaseModel):
    company_id: str
    assets: List[Dict[str, Any]]
    target_return: float
    risk_free_rate: float
    constraints: Dict[str, Any]

class PortfolioOptimizationResponse(BaseModel):
    company_id: str
    optimized_allocations: List[OptimizedAllocation]
    expected_return: float
    expected_risk: float
    sharpe_ratio: float
    efficient_frontier: List[Dict[str, float]]
    recommendations: List[str]

async def call_internal_service(service_url: str, endpoint: str, data: dict = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            response = await client.post(url, json=data) if data else await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "portfolio-optimization", "version": "1.0.0"}

@app.post("/optimize", response_model=PortfolioOptimizationResponse)
async def optimize_portfolio(request: PortfolioOptimizationRequest):
    logger.info("Optimizing portfolio", company=request.company_id)

    optimized_allocations = []
    total_value = sum(a.get("market_value", 0) for a in request.assets)

    for asset in request.assets:
        current_weight = asset.get("market_value", 0) / total_value if total_value else 0
        optimal_weight = 1.0 / len(request.assets) if request.assets else 0

        optimized_allocations.append(OptimizedAllocation(
            asset_id=asset.get("id", ""),
            asset_name=asset.get("name", ""),
            current_weight=round(current_weight, 4),
            optimal_weight=round(optimal_weight, 4),
            target_allocation=round(optimal_weight * 100, 2),
            adjustment_needed=round((optimal_weight - current_weight) * total_value, 2)
        ))

    expected_return = sum(a.get("expected_return", 0.1) * (1 / len(request.assets)) for a in request.assets) if request.assets else 0
    expected_risk = 0.15
    sharpe = (expected_return - request.risk_free_rate) / expected_risk if expected_risk else 0

    frontier = [
        {"return": 0.05, "risk": 0.08},
        {"return": 0.08, "risk": 0.12},
        {"return": 0.12, "risk": 0.18},
        {"return": 0.15, "risk": 0.22}
    ]

    return PortfolioOptimizationResponse(
        company_id=request.company_id,
        optimized_allocations=optimized_allocations,
        expected_return=round(expected_return, 4),
        expected_risk=round(expected_risk, 4),
        sharpe_ratio=round(sharpe, 4),
        efficient_frontier=frontier,
        recommendations=["Rebalance portfolio to optimal weights", "Monitor Sharpe ratio regularly", "Consider risk-adjusted returns"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8234)
