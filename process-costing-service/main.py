"""
Process Costing Service
Port: 8180
Process costing for manufacturing, equivalent units
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Process Costing Service", version="1.0.0")

class ProcessCostingRequest(BaseModel):
    company_id: str
    process_name: str
    opening_wip: float
    opening_wip_units: int
    units_started: int
    units_completed: int
    closing_wip_units: int
    closing_wip_completion: float
    direct_material_cost: float
    conversion_cost: float

class ProcessCostingResponse(BaseModel):
    company_id: str
    process_name: str
    equivalent_units_material: float
    equivalent_units_conversion: float
    cost_per_equivalent_unit_material: float
    cost_per_equivalent_unit_conversion: float
    cost_of_completed_units: float
    cost_of_closing_wip: float
    total_cost_accounted: float

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
    return {"status": "healthy", "service": "process-costing", "version": "1.0.0"}

@app.post("/calculate", response_model=ProcessCostingResponse)
async def calculate_process_cost(request: ProcessCostingRequest):
    logger.info("Calculating process cost", company=request.company_id, process=request.process_name)

    equivalent_units_material = request.units_completed + request.closing_wip_units * request.closing_wip_completion
    equivalent_units_conversion = equivalent_units_material

    total_material_cost = request.opening_wip + request.direct_material_cost
    total_conversion_cost = request.opening_wip + request.conversion_cost

    cost_per_unit_material = total_material_cost / equivalent_units_material if equivalent_units_material else 0
    cost_per_unit_conversion = total_conversion_cost / equivalent_units_conversion if equivalent_units_conversion else 0

    cost_completed = request.units_completed * (cost_per_unit_material + cost_per_unit_conversion)
    cost_closing_wip = request.closing_wip_units * request.closing_wip_completion * (cost_per_unit_material + cost_per_unit_conversion)

    return ProcessCostingResponse(
        company_id=request.company_id,
        process_name=request.process_name,
        equivalent_units_material=round(equivalent_units_material, 2),
        equivalent_units_conversion=round(equivalent_units_conversion, 2),
        cost_per_equivalent_unit_material=round(cost_per_unit_material, 2),
        cost_per_equivalent_unit_conversion=round(cost_per_unit_conversion, 2),
        cost_of_completed_units=round(cost_completed, 2),
        cost_of_closing_wip=round(cost_closing_wip, 2),
        total_cost_accounted=round(cost_completed + cost_closing_wip, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8180)
