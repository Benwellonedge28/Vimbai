"""
Vimbai Labour Cost Variance Service
Calculates total labour cost variance.
Total Labour Variance = Standard Labour Cost - Actual Labour Cost
Breakdown: Rate Variance + Efficiency Variance
"""

import os
import uuid
from datetime import datetime
from typing:Any, Dict

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "labour-cost-variance-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8124"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Labour Cost Variance Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Labour cost variance calculation"}


@app.post("/calculate")
async def calculate_labour_cost_variance(
    standard_rate: float,
    actual_rate: float,
    standard_hours: float,
    actual_hours: float
):
    """
    Calculate Total Labour Cost Variance.
    Total = (SR × SH) - (AR × AH)
    Rate Variance = (SR - AR) × AH
    Efficiency Variance = (SH - AH) × SR
    """
    standard_cost = standard_rate * standard_hours
    actual_cost = actual_rate * actual_hours
    total_variance = standard_cost - actual_cost

    # Rate variance
    rate_variance = (standard_rate - actual_rate) * actual_hours

    # Efficiency variance
    efficiency_variance = (standard_hours - actual_hours) * standard_rate

    return {
        "standard_rate": standard_rate,
        "actual_rate": actual_rate,
        "standard_hours": standard_hours,
        "actual_hours": actual_hours,
        "standard_cost": standard_cost,
        "actual_cost": actual_cost,
        "total_labour_cost_variance": round(total_variance, 2),
        "rate_variance": round(rate_variance, 2),
        "efficiency_variance": round(efficiency_variance, 2),
        "variance_sum_check": round(rate_variance + efficiency_variance, 2),
        "overall": "Favorable" if total_variance > 0 else "Adverse" if total_variance < 0 else "None"
    }


@app.post("/simple")
async def simple_labour_variance(standard_cost: float, actual_cost: float):
    """Simple labour variance calculation."""
    variance = standard_cost - actual_cost
    return {
        "standard_cost": standard_cost,
        "actual_cost": actual_cost,
        "variance": round(variance, 2),
        "type": "Favorable" if variance > 0 else "Adverse"
    }


@app.post("/multi-worker")
async def multi_worker_variance(workers: list):
    """Calculate variance for multiple workers."""
    total_std = 0
    total_actual = 0
    breakdown = []

    for w in workers:
        std = w.get("standard_rate", 0) * w.get("standard_hours", 0)
        act = w.get("actual_rate", 0) * w.get("actual_hours", 0)
        var = std - act
        total_std += std
        total_actual += act

        breakdown.append({
            "worker": w.get("name", "Unknown"),
            "standard_cost": std,
            "actual_cost": act,
            "variance": round(var, 2)
        })

    total_variance = total_std - total_actual

    return {
        "workers": breakdown,
        "total_standard_cost": total_std,
        "total_actual_cost": total_actual,
        "total_variance": round(total_variance, 2)
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
