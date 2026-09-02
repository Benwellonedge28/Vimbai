"""
Vimbai Labour Efficiency Variance Service
Detailed labour efficiency analysis with idle time and capacity variances.
Port: 8344
"""
import os, uuid
from typing import Dict, List
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "labour-efficiency-variance-service"
PORT = int(os.getenv("PORT", "8344"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Labour Efficiency Variance Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class EfficiencyData(BaseModel):
    department: str; standard_rate: float
    standard_hours_per_unit: float; actual_hours: float; actual_output: int
    idle_time_hours: float = 0

class EfficiencyRequest(BaseModel):
    company_id: str; period: str; departments: List[EfficiencyData]

class EfficiencyResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; period: str
    total_efficiency_variance: float; total_idle_time_variance: float
    total_productive_variance: float; departments: List[Dict] = []

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/analyze", response_model=EfficiencyResult)
async def analyze_efficiency(req: EfficiencyRequest):
    total_eff = 0; total_idle = 0; total_prod = 0
    dept_results = []
    
    for dept in req.departments:
        std_hours = dept.standard_hours_per_unit * dept.actual_output
        eff_variance = (dept.actual_hours - std_hours) * dept.standard_rate
        idle_variance = dept.idle_time_hours * dept.standard_rate
        productive_variance = (dept.actual_hours - dept.idle_time_hours - std_hours) * dept.standard_rate
        
        total_eff += eff_variance; total_idle += idle_variance; total_prod += productive_variance
        dept_results.append({
            "department": dept.department,
            "standard_hours": round(std_hours, 2), "actual_hours": dept.actual_hours,
            "idle_time_hours": dept.idle_time_hours,
            "efficiency_variance": round(eff_variance, 2),
            "idle_time_variance": round(idle_variance, 2),
            "productive_efficiency_variance": round(productive_variance, 2),
            "output_units": dept.actual_output,
            "favorable": eff_variance < 0
        })
    
    return EfficiencyResult(
        company_id=req.company_id, period=req.period,
        total_efficiency_variance=round(total_eff, 2),
        total_idle_time_variance=round(total_idle, 2),
        total_productive_variance=round(total_prod, 2),
        departments=dept_results
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
