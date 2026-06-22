"""
Target Costing Service
Port: 8183
Market-based target costing, allowed cost calculation
"""
import httpx
import structlog
from typing import Any, Dict
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Target Costing Service", version="1.0.0")

class TargetCostingRequest(BaseModel):
    company_id: str
    product_name: str
    market_price: float
    target_profit_margin: float
    current_estimated_cost: float

class TargetCostingResponse(BaseModel):
    company_id: str
    product_name: str
    market_price: float
    target_profit: float
    allowed_cost: float
    current_cost: float
    cost_reduction_needed: float
    reduction_percentage: float

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
    return {"status": "healthy", "service": "target-costing", "version": "1.0.0"}

@app.post("/calculate", response_model=TargetCostingResponse)
async def calculate_target_cost(request: TargetCostingRequest):
    logger.info("Calculating target cost", company=request.company_id, product=request.product_name)

    target_profit = request.market_price * (request.target_profit_margin / 100)
    allowed_cost = request.market_price - target_profit
    reduction_needed = request.current_estimated_cost - allowed_cost
    reduction_pct = (reduction_needed / request.current_estimated_cost) * 100 if request.current_estimated_cost else 0

    return TargetCostingResponse(
        company_id=request.company_id,
        product_name=request.product_name,
        market_price=request.market_price,
        target_profit=round(target_profit, 2),
        allowed_cost=round(allowed_cost, 2),
        current_cost=request.current_estimated_cost,
        cost_reduction_needed=round(max(0, reduction_needed), 2),
        reduction_percentage=round(reduction_pct, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8183)
