"""
Lifecycle Costing Service
Port: 8182
Product lifecycle costs from R&D to disposal
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Lifecycle Costing Service", version="1.0.0")

class LifecycleCostRequest(BaseModel):
    company_id: str
    product_name: str
    rd_costs: float
    design_costs: float
    production_costs: float
    marketing_costs: float
    distribution_costs: float
    customer_service_costs: float
    disposal_costs: float
    expected_units: int
    product_life_years: int

class LifecycleCostingResponse(BaseModel):
    company_id: str
    product_name: str
    rd_phase: float
    design_phase: float
    production_phase: float
    marketing_phase: float
    distribution_phase: float
    post_production_phase: float
    total_lifecycle_cost: float
    cost_per_unit: float
    annual_cost: float

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
    return {"status": "healthy", "service": "lifecycle-costing", "version": "1.0.0"}

@app.post("/calculate", response_model=LifecycleCostingResponse)
async def calculate_lifecycle_cost(request: LifecycleCostRequest):
    logger.info("Calculating lifecycle cost", company=request.company_id, product=request.product_name)

    rd_phase = request.rd_costs
    design_phase = request.design_costs
    production_phase = request.production_costs * request.expected_units
    marketing_phase = request.marketing_costs
    distribution_phase = request.distribution_costs * request.expected_units
    post_production = request.customer_service_costs + request.disposal_costs

    total_lifecycle = rd_phase + design_phase + production_phase + marketing_phase + distribution_phase + post_production
    cost_per_unit = total_lifecycle / request.expected_units if request.expected_units else 0
    annual_cost = total_lifecycle / request.product_life_years if request.product_life_years else 0

    return LifecycleCostingResponse(
        company_id=request.company_id,
        product_name=request.product_name,
        rd_phase=round(rd_phase, 2),
        design_phase=round(design_phase, 2),
        production_phase=round(production_phase, 2),
        marketing_phase=round(marketing_phase, 2),
        distribution_phase=round(distribution_phase, 2),
        post_production_phase=round(post_production, 2),
        total_lifecycle_cost=round(total_lifecycle, 2),
        cost_per_unit=round(cost_per_unit, 2),
        annual_cost=round(annual_cost, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8182)
