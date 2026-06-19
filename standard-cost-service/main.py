"""
FinAcc Standard Cost Service
Handles standard cost calculations.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "standard-cost-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8110"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Standard Cost Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Standard cost calculations"}


@app.post("/material-standard")
async def calculate_material_standard_cost(
    standard_price_per_unit: float,
    standard_quantity: float
):
    """Calculate standard material cost."""
    standard_cost = standard_price_per_unit * standard_quantity
    return {
        "standard_price_per_unit": standard_price_per_unit,
        "standard_quantity": standard_quantity,
        "standard_material_cost": standard_cost
    }


@app.post("/labour-standard")
async def calculate_labour_standard_cost(
    standard_rate_per_hour: float,
    standard_hours: float
):
    """Calculate standard labour cost."""
    standard_cost = standard_rate_per_hour * standard_hours
    return {
        "standard_rate_per_hour": standard_rate_per_hour,
        "standard_hours": standard_hours,
        "standard_labour_cost": standard_cost
    }


@app.post("/overhead-standard")
async def calculate_overhead_standard_cost(
    overhead_rate: float,
    standard_activity_level: float
):
    """Calculate standard overhead cost."""
    standard_cost = overhead_rate * standard_activity_level
    return {
        "overhead_rate": overhead_rate,
        "standard_activity_level": standard_activity_level,
        "standard_overhead_cost": standard_cost
    }


@app.post("/total-standard")
async def calculate_total_standard_cost(
    standard_direct_material: float,
    standard_direct_labour: float,
    standard_overhead: float
):
    """Calculate total standard cost."""
    total = standard_direct_material + standard_direct_labour + standard_overhead
    return {
        "standard_direct_material": standard_direct_material,
        "standard_direct_labour": standard_direct_labour,
        "standard_overhead": standard_overhead,
        "total_standard_cost": total
    }


@app.post("/per-unit")
async def calculate_standard_cost_per_unit(
    standard_material_cost: float,
    standard_labour_cost: float,
    standard_overhead: float,
    units_produced: float
):
    """Calculate standard cost per unit."""
    total_standard = standard_material_cost + standard_labour_cost + standard_overhead
    cost_per_unit = total_standard / units_produced if units_produced > 0 else 0

    return {
        "standard_material_cost": standard_material_cost,
        "standard_labour_cost": standard_labour_cost,
        "standard_overhead": standard_overhead,
        "total_standard_cost": total_standard,
        "units_produced": units_produced,
        "standard_cost_per_unit": round(cost_per_unit, 2)
    }


@app.post("/variance-analysis")
async def standard_cost_variance_analysis(
    standard_cost: float,
    actual_cost: float
):
    """Analyze variance between standard and actual cost."""
    variance = standard_cost - actual_cost
    return {
        "standard_cost": standard_cost,
        "actual_cost": actual_cost,
        "variance": variance,
        "interpretation": "Favorable" if variance > 0 else "Adverse" if variance < 0 else "None"
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
