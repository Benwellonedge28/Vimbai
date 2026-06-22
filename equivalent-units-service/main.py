"""
Equivalent Units Service
Port: 8186
Calculate equivalent units for work in process
"""
import httpx
import structlog
from typing import Any, Dict
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Equivalent Units Service", version="1.0.0")

class EquivalentUnitsRequest(BaseModel):
    company_id: str
    process_id: str
    units_completed: int
    closing_wip_units: int
    completion_percentage: float
    method: str = "weighted_average"

class EquivalentUnitsResponse(BaseModel):
    company_id: str
    process_id: str
    equivalent_units: float
    cost_per_equivalent_unit: float
    cost_of_completed_units: float
    cost_of_closing_wip: float

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
    return {"status": "healthy", "service": "equivalent-units", "version": "1.0.0"}

@app.post("/calculate", response_model=EquivalentUnitsResponse)
async def calculate_equivalent_units(request: EquivalentUnitsRequest):
    logger.info("Calculating equivalent units", company=request.company_id)

    equivalent_units = request.units_completed + request.closing_wip_units * request.completion_percentage / 100
    cost_per_unit = 50.0

    return EquivalentUnitsResponse(
        company_id=request.company_id,
        process_id=request.process_id,
        equivalent_units=round(equivalent_units, 2),
        cost_per_equivalent_unit=round(cost_per_unit, 2),
        cost_of_completed_units=round(request.units_completed * cost_per_unit, 2),
        cost_of_closing_wip=round(request.closing_wip_units * request.completion_percentage / 100 * cost_per_unit, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8186)
