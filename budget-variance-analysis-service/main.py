"""
Budget Variance Analysis Service
Port: 8174
Flexible budget analysis, variance decomposition
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Budget Variance Analysis Service", version="1.0.0")

class VarianceRequest(BaseModel):
    company_id: str
    budgeted_amount: float
    actual_amount: float
    budgeted_volume: int
    actual_volume: int
    item_name: str

class VarianceResponse(BaseModel):
    item_name: str
    total_variance: float
    variance_percentage: float
    price_variance: float
    quantity_variance: float
    volume_variance: float
    is_favorable: bool

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
    return {"status": "healthy", "service": "budget-variance-analysis", "version": "1.0.0"}

@app.post("/analyze", response_model=VarianceResponse)
async def analyze_variance(request: VarianceRequest):
    logger.info("Analyzing budget variance", company=request.company_id, item=request.item_name)

    total_variance = request.actual_amount - request.budgeted_amount
    variance_pct = (total_variance / request.budgeted_amount) * 100 if request.budgeted_amount else 0

    budgeted_price = request.budgeted_amount / request.budgeted_volume if request.budgeted_volume else 0
    actual_price = request.actual_amount / request.actual_volume if request.actual_volume else 0
    standard_cost_actual_volume = budgeted_price * request.actual_volume

    price_variance = (actual_price - budgeted_price) * request.actual_volume
    quantity_variance = (request.actual_volume - request.budgeted_volume) * budgeted_price
    volume_variance = quantity_variance

    return VarianceResponse(
        item_name=request.item_name,
        total_variance=round(total_variance, 2),
        variance_percentage=round(variance_pct, 2),
        price_variance=round(price_variance, 2),
        quantity_variance=round(quantity_variance, 2),
        volume_variance=round(volume_variance, 2),
        is_favorable=total_variance < 0
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8174)
