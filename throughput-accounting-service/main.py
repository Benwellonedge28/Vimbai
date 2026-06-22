"""
Throughput Accounting Service
Port: 8184
Theory of constraints, throughput, inventory, operating expense analysis
"""
import httpx
import structlog
from typing import Any, Dict
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Throughput Accounting Service", version="1.0.0")

class ThroughputRequest(BaseModel):
    company_id: str
    revenue: float
    direct_material_cost: float
    operating_expenses: float
    throughput_per_hour: float
    constraint_hours_available: int

class ThroughputResponse(BaseModel):
    company_id: str
    throughput: float
    total_inventory: float
    total_operating_expense: float
    throughput_accounting_ratio: float
    roi: float
    productivity: float

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
    return {"status": "healthy", "service": "throughput-accounting", "version": "1.0.0"}

@app.post("/calculate", response_model=ThroughputResponse)
async def calculate_throughput(request: ThroughputRequest):
    logger.info("Calculating throughput", company=request.company_id)

    throughput = request.revenue - request.direct_material_cost
    inventory = request.direct_material_cost
    tddr = throughput / request.operating_expenses if request.operating_expenses else 0
    roi = (throughput - request.operating_expenses) / inventory if inventory else 0
    productivity = request.throughput_per_hour * request.constraint_hours_available

    return ThroughputResponse(
        company_id=request.company_id,
        throughput=round(throughput, 2),
        total_inventory=round(inventory, 2),
        total_operating_expense=request.operating_expenses,
        throughput_accounting_ratio=round(tddr, 2),
        roi=round(roi, 2),
        productivity=round(productivity, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8184)
