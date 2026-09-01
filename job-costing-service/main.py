"""Vimbai Job Costing Service - Track costs per job/project. Port: 8361"""
import os, uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "job-costing-service"
PORT = int(os.getenv("PORT", "8361"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Job Costing Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="job-costing-service", instrument_app=app)
except ImportError:
    TRACER = None

class JobCostEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    job_id: str
    cost_type: str  # materials, labor, overhead, subcontractor
    amount: float
    date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    description: str = ""

class Job(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    job_name: str
    customer: str = ""
    contract_value: float = 0
    status: str = "active"  # active, completed, billed, closed
    start_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    end_date: Optional[datetime] = None
    materials_cost: float = 0
    labor_cost: float = 0
    overhead_cost: float = 0
    subcontractor_cost: float = 0
    total_cost: float = 0
    gross_profit: float = 0
    gross_margin: float = 0

_jobs: Dict[str, List[Job]] = defaultdict(list)
_entries: Dict[str, List[JobCostEntry]] = defaultdict(list)

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/jobs", response_model=Job)
async def create_job(job: Job):
    _jobs[job.company_id].append(job)
    return job

@app.get("/jobs/{company_id}")
async def get_jobs(company_id: str, status: Optional[str] = None):
    jobs = _jobs.get(company_id, [])
    if status: jobs = [j for j in jobs if j.status == status]
    return {"company_id": company_id, "jobs": jobs, "total": len(jobs)}

@app.post("/jobs/{job_id}/costs")
async def add_cost(job_id: str, entry: JobCostEntry):
    entry.job_id = job_id
    _entries[job_id].append(entry)
    # Update job
    for jobs in _jobs.values():
        for j in jobs:
            if j.id == job_id:
                if entry.cost_type == "materials": j.materials_cost += entry.amount
                elif entry.cost_type == "labor": j.labor_cost += entry.amount
                elif entry.cost_type == "overhead": j.overhead_cost += entry.amount
                elif entry.cost_type == "subcontractor": j.subcontractor_cost += entry.amount
                j.total_cost = j.materials_cost + j.labor_cost + j.overhead_cost + j.subcontractor_cost
                j.gross_profit = j.contract_value - j.total_cost
                j.gross_margin = (j.gross_profit / max(1, j.contract_value)) * 100
                return {"job_id": job_id, "total_cost": j.total_cost, "gross_profit": j.gross_profit, "margin": j.gross_margin}
    raise HTTPException(status_code=404, detail="Job not found")

@app.get("/jobs/{company_id}/profitability")
async def job_profitability(company_id: str):
    jobs = _jobs.get(company_id, [])
    completed = [j for j in jobs if j.status in ("completed", "closed")]
    return {"company_id": company_id, "total_jobs": len(jobs), "completed": len(completed), "total_contract_value": sum(j.contract_value for j in jobs), "total_cost": sum(j.total_cost for j in jobs), "total_profit": sum(j.gross_profit for j in jobs), "avg_margin": sum(j.gross_margin for j in jobs) / max(1, len(jobs))}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
