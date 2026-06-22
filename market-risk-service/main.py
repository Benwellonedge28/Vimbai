"""
Market Risk Service
Port: 8165
Value at Risk (VaR), stress testing, sensitivity analysis
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Market Risk Service", version="1.0.0")

class Position(BaseModel):
    position_id: str
    asset_class: str
    notional: float
    market_value: float
    volatility: float

class MarketRiskRequest(BaseModel):
    company_id: str
    positions: List[Position]
    confidence_level: float = Field(default=0.99, ge=0.9, le=0.999)
    holding_period_days: int = 10

class MarketRiskResponse(BaseModel):
    company_id: str
    var_absolute: float
    var_percentage: float
    expected_shortfall: float
    stressed_var: float
    risk_decomposition: Dict[str, float]

async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
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
    return {"status": "healthy", "service": "market-risk", "version": "1.0.0"}

@app.post("/calculate", response_model=MarketRiskResponse)
async def calculate_market_risk(request: MarketRiskRequest):
    logger.info("Calculating market risk", company=request.company_id, positions=len(request.positions))

    total_value = sum(p.market_value for p in request.positions)
    z_score = 2.33 if request.confidence_level == 0.99 else 1.65

    var = total_value * max(p.volatility for p in request.positions) * z_score * (request.holding_period_days ** 0.5) / 100
    expected_shortfall = var * 1.2
    stressed_var = var * 1.5

    decomposition = {}
    for p in request.positions:
        decomposition[p.asset_class] = var * (p.market_value / total_value)

    return MarketRiskResponse(
        company_id=request.company_id,
        var_absolute=var,
        var_percentage=var / total_value * 100 if total_value > 0 else 0,
        expected_shortfall=expected_shortfall,
        stressed_var=stressed_var,
        risk_decomposition=decomposition
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8165)
