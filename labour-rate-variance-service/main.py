"""
Vimbai Labour Rate Variance Service
Calculates labour rate/wages variance.
Rate Variance = (Standard Rate - Actual Rate) × Actual Hours
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "labour-rate-variance-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8122"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Labour Rate Variance Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Labour rate variance calculation"}


@app.post("/calculate")
async def calculate_rate_variance(
    standard_rate: float,
    actual_rate: float,
    actual_hours: float
):
    """
    Calculate Labour Rate Variance.
    Formula: (SR - AR) × AH
    Favorable = SR > AR (paid less than standard rate)
    Adverse = SR < AR (paid more than standard rate)
    """
    variance = (standard_rate - actual_rate) * actual_hours
    variance_type = "Favorable" if variance > 0 else "Adverse" if variance < 0 else "None"

    return {
        "standard_rate": standard_rate,
        "actual_rate": actual_rate,
        "actual_hours": actual_hours,
        "rate_variance": round(variance, 2),
        "variance_type": variance_type,
        "formula": f"({standard_rate} - {actual_rate}) × {actual_hours} = {variance}"
    }


@app.post("/wages-variance")
async def calculate_wages_variance(
    standard_wage_rate: float,
    actual_wage_rate: float,
    hours_worked: float
):
    """Calculate wages rate variance."""
    variance = (standard_wage_rate - actual_wage_rate) * hours_worked
    return {
        "standard_wage_rate": standard_wage_rate,
        "actual_wage_rate": actual_wage_rate,
        "hours_worked": hours_worked,
        "wages_variance": round(variance, 2),
        "type": "Favorable" if variance > 0 else "Adverse"
    }


@app.post("/total-rate-variance")
async def total_rate_variance(
    standard_rate_per_hour: float,
    actual_rate_per_hour: float,
    total_hours_worked: float
):
    """Calculate total labour rate variance."""
    variance = (standard_rate_per_hour - actual_rate_per_hour) * total_hours_worked
    std_total = standard_rate_per_hour * total_hours_worked
    actual_total = actual_rate_per_hour * total_hours_worked

    return {
        "standard_rate": standard_rate_per_hour,
        "actual_rate": actual_rate_per_hour,
        "total_hours": total_hours_worked,
        "standard_cost": std_total,
        "actual_cost": actual_total,
        "rate_variance": round(variance, 2),
        "interpretation": "Lower rates paid" if variance > 0 else "Higher rates paid"
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
