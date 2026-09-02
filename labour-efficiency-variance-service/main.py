"""
Vimbai Labour Efficiency Variance Service
Standard costing labour variances: rate, efficiency, and idle time analysis.
Port: 8400
"""
import os, uuid
from typing import Dict, List
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "labour-efficiency-variance-service"
PORT = int(os.getenv("PORT", "8400"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Labour Efficiency Variance Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class LabourVarianceRequest(BaseModel):
    company_id: str; department: str; period: str
    standard_hours: float; standard_rate: float
    actual_hours: float; actual_rate: float
    actual_output: float; standard_hours_per_unit: float = 1.0
    idle_hours: float = 0

class LabourVarianceResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; department: str; period: str
    rate_variance: float; efficiency_variance: float
    idle_time_variance: float; total_variance: float
    favourable: bool
    analysis: Dict[str, str] = {}

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/calculate", response_model=LabourVarianceResult)
async def calculate_variance(req: LabourVarianceRequest):
    expected_hours = req.actual_output * req.standard_hours_per_unit
    
    rate_var = (req.standard_rate - req.actual_rate) * req.actual_hours
    eff_var = (req.standard_hours - req.actual_hours) * req.standard_rate if req.standard_hours else \
              (expected_hours - req.actual_hours) * req.standard_rate
    idle_var = -(req.idle_hours * req.standard_rate)
    
    total = rate_var + eff_var + idle_var
    favourable = total >= 0
    
    analysis = {}
    if rate_var < 0:
        analysis["rate"] = f"Unfavourable: paying {req.actual_rate} vs standard {req.standard_rate} - negotiate or review pay scales"
    else:
        analysis["rate"] = f"Favourable: paying below standard rate"
    if eff_var < 0:
        analysis["efficiency"] = f"Unfavourable: {req.actual_hours}h actual vs {expected_hours}h expected - training or process improvement needed"
    else:
        analysis["efficiency"] = f"Favourable: fewer hours than standard"
    if idle_var < 0:
        analysis["idle_time"] = f"Unfavourable: {req.idle_hours}h idle time costing {abs(idle_var)}"
    
    return LabourVarianceResult(
        company_id=req.company_id, department=req.department, period=req.period,
        rate_variance=round(rate_var, 2), efficiency_variance=round(eff_var, 2),
        idle_time_variance=round(idle_var, 2), total_variance=round(total, 2),
        favourable=favourable, analysis=analysis
    )

# Backward-compatible /analyze endpoint
class DeptAnalysis(BaseModel):
    department: str; standard_rate: float = 20
    standard_hours_per_unit: float = 1.0
    actual_hours: float = 0; actual_output: float = 0
    idle_time_hours: float = 0

class AnalysisRequestCompat(BaseModel):
    company_id: str; period: str
    departments: List[DeptAnalysis] = []

@app.post("/analyze")
async def analyze_departments(req: AnalysisRequestCompat):
    total_idle_var = 0
    total_eff_var = 0
    dept_results = []
    for d in req.departments:
        expected = d.actual_output * d.standard_hours_per_unit
        idle_var = -(d.idle_time_hours * d.standard_rate)
        eff_var = (expected - d.actual_hours) * d.standard_rate
        total_idle_var += abs(idle_var)
        total_eff_var += eff_var
        dept_results.append({
            "department": d.department,
            "idle_time_variance": round(abs(idle_var), 2),
            "efficiency_variance": round(eff_var, 2),
            "idle_time_hours": d.idle_time_hours
        })
    return {
        "total_idle_time_variance": round(total_idle_var, 2),
        "total_efficiency_variance": round(total_eff_var, 2),
        "departments": dept_results
    }

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
