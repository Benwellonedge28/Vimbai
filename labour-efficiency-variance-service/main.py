"""
FinAcc Labour Efficiency Variance Service
Calculates labour efficiency/idle time variance.
Efficiency Variance = (Standard Hours - Actual Hours) × Standard Rate
"""

import os
import uuid
from datetime import datetime
from typing:Any, Dict

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "labour-efficiency-variance-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8123"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Labour Efficiency Variance Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Labour efficiency variance calculation"}


@app.post("/calculate")
async def calculate_efficiency_variance(
    standard_hours: float,
    actual_hours: float,
    standard_rate: float
):
    """
    Calculate Labour Efficiency Variance.
    Formula: (SH - AH) × SR
    Favorable = SH > AH (more efficient - took less time)
    Adverse = SH < AH (less efficient - took more time)
    """
    variance = (standard_hours - actual_hours) * standard_rate
    hours_diff = actual_hours - standard_hours
    variance_type = "Favorable" if variance > 0 else "Adverse" if variance < 0 else "None"

    return {
        "standard_hours": standard_hours,
        "actual_hours": actual_hours,
        "standard_rate": standard_rate,
        "hours_difference": hours_diff,
        "efficiency_variance": round(variance, 2),
        "variance_type": variance_type,
        "formula": f"({standard_hours} - {actual_hours}) × {standard_rate} = {variance}"
    }


@app.post("/per-unit")
async def calculate_efficiency_per_unit(
    standard_hours_per_unit: float,
    units_produced: float,
    actual_hours: float,
    standard_rate: float
):
    """Calculate efficiency variance per unit produced."""
    std_hours_total = standard_hours_per_unit * units_produced
    variance = (std_hours_total - actual_hours) * standard_rate

    return {
        "standard_hours_per_unit": standard_hours_per_unit,
        "units_produced": units_produced,
        "total_standard_hours": std_hours_total,
        "actual_hours": actual_hours,
        "standard_rate": standard_rate,
        "efficiency_variance": round(variance, 2),
        "type": "Favorable" if variance > 0 else "Adverse"
    }


@app.post("/idle-time")
async def calculate_idle_time_variance(
    standard_hours: float,
    idle_hours: float,
    standard_rate: float
):
    """Calculate idle time variance."""
    # When actual hours > standard hours due to idle time
    effective_hours = standard_hours + idle_hours
    variance = idle_hours * standard_rate

    return {
        "standard_hours": standard_hours,
        "idle_hours": idle_hours,
        "total_hours_worked": effective_hours,
        "standard_rate": standard_rate,
        "idle_time_variance": round(variance, 2),
        "type": "Adverse",
        "note": "Idle time is always an adverse variance"
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
