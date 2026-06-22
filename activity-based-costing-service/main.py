"""
Activity-Based Costing Service
Port: 8179
ABC cost pools, activity drivers, unit-level/batch-level costs
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Activity-Based Costing Service", version="1.0.0")

class ABCActivity(BaseModel):
    activity_id: str
    activity_name: str
    activity_type: str
    cost_pool: float
    cost_driver: str
    driver_volume: int

class ABCProduct(BaseModel):
    product_id: str
    product_name: str
    unit_level_drivers: Dict[str, int]
    batch_level_drivers: Dict[str, int]
    product_level_activities: Dict[str, float]

class ABCRequest(BaseModel):
    company_id: str
    activities: List[ABCActivity]
    products: List[ABCProduct]

class ABCResponse(BaseModel):
    company_id: str
    activity_rates: List[Dict[str, Any]]
    product_costs: List[Dict[str, Any]]
    unit_costs: Dict[str, float]
    total_overhead_allocated: float

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
    return {"status": "healthy", "service": "activity-based-costing", "version": "1.0.0"}

@app.post("/calculate", response_model=ABCResponse)
async def calculate_abc(request: ABCRequest):
    logger.info("Calculating ABC", company=request.company_id)

    activity_rates = []
    for activity in request.activities:
        rate = activity.cost_pool / activity.driver_volume if activity.driver_volume else 0
        activity_rates.append({
            "activity_id": activity.activity_id,
            "activity_name": activity.activity_name,
            "activity_type": activity.activity_type,
            "rate_per_driver": round(rate, 2)
        })

    product_costs = []
    total_overhead = 0

    for product in request.products:
        unit_cost = sum(product.unit_level_drivers.get(a.activity_id, 0) *
                       (a.cost_pool / a.driver_volume if a.driver_volume else 0)
                       for a in request.activities if a.activity_type == "unit_level")

        batch_cost = sum(product.batch_level_drivers.get(a.activity_id, 0) *
                        (a.cost_pool / a.driver_volume if a.driver_volume else 0)
                        for a in request.activities if a.activity_type == "batch_level")

        product_cost = unit_cost + batch_cost + sum(product.product_level_activities.values())
        total_overhead += product_cost

        product_costs.append({
            "product_id": product.product_id,
            "product_name": product.product_name,
            "unit_level_cost": round(unit_cost, 2),
            "batch_level_cost": round(batch_cost, 2),
            "product_level_cost": round(sum(product.product_level_activities.values()), 2),
            "total_product_cost": round(product_cost, 2)
        })

    return ABCResponse(
        company_id=request.company_id,
        activity_rates=activity_rates,
        product_costs=product_costs,
        unit_costs={p.product_id: round(sum(p.unit_level_drivers.values()), 2) for p in request.products},
        total_overhead_allocated=total_overhead
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8179)
