"""Job Costing Service - Port 8341"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Job Costing Service", version="1.0.0")

class JobCostingRequest(BaseModel):
    company_id: str; jobs: list

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "job-costing"}

@app.post("/analyze", response_model=dict)
async def analyze_jobs(request: JobCostingRequest):
    total_job_cost = sum(j.get("cost", 0) for j in request.jobs)
    return {"company_id": request.company_id, "total_jobs": len(request.jobs), "total_cost": total_job_cost}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8341)
