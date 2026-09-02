"""
Vimbai Labour Cost Variance Service
Calculates labour rate and efficiency variances for cost control.
Port: 8343
"""

import os
import uuid
from typing import Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "labour-cost-variance-service"
PORT = int(os.getenv("PORT", "8343"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Labour Cost Variance Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class LabourData(BaseModel):
    department: str
    standard_rate: float
    actual_rate: float
    standard_hours: float
    actual_hours: float
    standard_output: float = 0
    actual_output: float = 0


class LabourVarianceRequest(BaseModel):
    company_id: str
    period: str
    departments: List[LabourData]


class LabourVarianceResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period: str
    total_rate_variance: float
    total_efficiency_variance: float
    total_cost_variance: float
    total_idle_time_variance: float
    departments: List[Dict] = []


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/analyze", response_model=LabourVarianceResult)
async def analyze_labour_variance(req: LabourVarianceRequest):
    total_rate = 0
    total_eff = 0
    total_cost = 0
    total_idle = 0
    dept_results = []

    for dept in req.departments:
        rate_variance = (dept.actual_rate - dept.standard_rate) * dept.actual_hours

        if dept.actual_output > 0 and dept.standard_output > 0:
            std_hours_for_actual = (dept.standard_hours / dept.standard_output) * dept.actual_output
        else:
            std_hours_for_actual = dept.standard_hours

        efficiency_variance = (dept.actual_hours - std_hours_for_actual) * dept.standard_rate
        cost_variance = rate_variance + efficiency_variance

        total_rate += rate_variance
        total_eff += efficiency_variance
        total_cost += cost_variance

        dept_results.append(
            {
                "department": dept.department,
                "rate_variance": round(rate_variance, 2),
                "efficiency_variance": round(efficiency_variance, 2),
                "total_cost_variance": round(cost_variance, 2),
                "standard_hours": dept.standard_hours,
                "actual_hours": dept.actual_hours,
                "standard_rate": dept.standard_rate,
                "actual_rate": dept.actual_rate,
                "favorable_rate": rate_variance < 0,
                "favorable_efficiency": efficiency_variance < 0,
            }
        )

    return LabourVarianceResult(
        company_id=req.company_id,
        period=req.period,
        total_rate_variance=round(total_rate, 2),
        total_efficiency_variance=round(total_eff, 2),
        total_cost_variance=round(total_cost, 2),
        total_idle_time_variance=0,
        departments=dept_results,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
