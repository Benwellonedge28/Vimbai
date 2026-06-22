"""
Job Costing Service
Port: 8181
Job order costing, job cost sheets, overhead allocation
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Job Costing Service", version="1.0.0")

class JobRequest(BaseModel):
    job_id: str
    direct_material_cost: float
    direct_labour_cost: float
    direct_labour_hours: float
    machine_hours: float
    overhead_rate: float
    overhead_allocation_method: str = "labour_hours"

class JobCostingRequest(BaseModel):
    company_id: str
    jobs: List[JobRequest]

class JobCostingResponse(BaseModel):
    company_id: str
    job_costs: List[Dict[str, Any]]
    total_job_cost: float
    average_job_cost: float

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
    return {"status": "healthy", "service": "job-costing", "version": "1.0.0"}

@app.post("/calculate", response_model=JobCostingResponse)
async def calculate_job_costs(request: JobCostingRequest):
    logger.info("Calculating job costs", company=request.company_id)

    job_costs = []
    total_cost = 0

    for job in request.jobs:
        if job.overhead_allocation_method == "labour_hours":
            overhead = job.overhead_rate * job.direct_labour_hours
        elif job.overhead_allocation_method == "machine_hours":
            overhead = job.overhead_rate * job.machine_hours
        else:
            overhead = job.overhead_rate * (job.direct_material_cost + job.direct_labour_cost) * 0.5

        prime_cost = job.direct_material_cost + job.direct_labour_cost
        total_job_cost = prime_cost + overhead
        total_cost += total_job_cost

        job_costs.append({
            "job_id": job.job_id,
            "direct_material": job.direct_material_cost,
            "direct_labour": job.direct_labour_cost,
            "prime_cost": prime_cost,
            "overhead_allocated": round(overhead, 2),
            "total_job_cost": round(total_job_cost, 2)
        })

    return JobCostingResponse(
        company_id=request.company_id,
        job_costs=job_costs,
        total_job_cost=round(total_cost, 2),
        average_job_cost=round(total_cost / len(request.jobs), 2) if request.jobs else 0
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8181)
