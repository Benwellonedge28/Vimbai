"""
Vimbai Cost Accounting Service
Job order and process costing with overhead allocation and variance analysis.
Port: 8402
"""
import os, uuid
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "cost-accounting-service"
PORT = int(os.getenv("PORT", "8402"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Cost Accounting Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class CostElement(BaseModel):
    description: str; amount: float; type: str = "direct_material"  # direct_material, direct_labour, overhead

class JobCostRequest(BaseModel):
    company_id: str; job_id: str; job_name: str
    direct_materials: float; direct_labour: float
    direct_labour_hours: float; overhead_rate: float
    additional_costs: List[CostElement] = []
    units_produced: int = 1

class JobCostResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; job_id: str; job_name: str
    direct_materials: float; direct_labour: float
    applied_overhead: float; additional_costs: float
    total_cost: float; cost_per_unit: float
    cost_breakdown: Dict[str, float] = {}

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/job-cost", response_model=JobCostResult)
async def calculate_job_cost(req: JobCostRequest):
    overhead = req.direct_labour_hours * req.overhead_rate
    additional = sum(c.amount for c in req.additional_costs)
    total = req.direct_materials + req.direct_labour + overhead + additional
    per_unit = total / req.units_produced if req.units_produced else total
    
    breakdown = {
        "direct_materials": round(req.direct_materials, 2),
        "direct_labour": round(req.direct_labour, 2),
        "applied_overhead": round(overhead, 2),
        "additional_costs": round(additional, 2),
        "total": round(total, 2)
    }
    
    return JobCostResult(
        company_id=req.company_id, job_id=req.job_id, job_name=req.job_name,
        direct_materials=round(req.direct_materials, 2),
        direct_labour=round(req.direct_labour, 2),
        applied_overhead=round(overhead, 2),
        additional_costs=round(additional, 2),
        total_cost=round(total, 2),
        cost_per_unit=round(per_unit, 2),
        cost_breakdown=breakdown
    )

# Backward-compatible /standards endpoint
class StandardCostReq(BaseModel):
    company_id: str; product_name: str
    direct_materials_std: float; direct_labor_std: float; overhead_std: float
    units_produced: int; actual_materials: float; actual_labor: float; actual_overhead: float

@app.post("/standards")
async def standard_costing(req: StandardCostReq):
    std_cost_per_unit = req.direct_materials_std + req.direct_labor_std + req.overhead_std
    actual_total = req.actual_materials + req.actual_labor + req.actual_overhead
    actual_cost_per_unit = actual_total / req.units_produced if req.units_produced else 0
    mat_var = req.actual_materials - (req.direct_materials_std * req.units_produced)
    lab_var = req.actual_labor - (req.direct_labor_std * req.units_produced)
    ovh_var = req.actual_overhead - (req.overhead_std * req.units_produced)
    total_var = mat_var + lab_var + ovh_var
    return {
        "standard_cost_per_unit": std_cost_per_unit,
        "actual_cost_per_unit": round(actual_cost_per_unit, 1),
        "material_variance": round(mat_var, 2),
        "labor_variance": round(lab_var, 2),
        "overhead_variance": round(ovh_var, 2),
        "total_variance": round(total_var, 2),
        "favorable": total_var <= 0
    }

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
