"""
Vimbai Actual Cost Service
Handles actual cost calculations and comparisons.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "actual-cost-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8112"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Actual Cost Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Actual cost calculations"}


@app.post("/material-actual")
async def calculate_actual_material_cost(
    actual_quantity: float,
    actual_price_per_unit: float
):
    """Calculate actual material cost."""
    actual_cost = actual_quantity * actual_price_per_unit
    return {
        "actual_quantity": actual_quantity,
        "actual_price_per_unit": actual_price_per_unit,
        "actual_material_cost": actual_cost
    }


@app.post("/labour-actual")
async def calculate_actual_labour_cost(
    actual_hours: float,
    actual_rate_per_hour: float
):
    """Calculate actual labour cost."""
    actual_cost = actual_hours * actual_rate_per_hour
    return {
        "actual_hours": actual_hours,
        "actual_rate_per_hour": actual_rate_per_hour,
        "actual_labour_cost": actual_cost
    }


@app.post("/total-actual")
async def calculate_total_actual_cost(
    actual_direct_material: float,
    actual_direct_labour: float,
    actual_overhead: float
):
    """Calculate total actual cost."""
    total = actual_direct_material + actual_direct_labour + actual_overhead
    return {
        "actual_direct_material": actual_direct_material,
        "actual_direct_labour": actual_direct_labour,
        "actual_overhead": actual_overhead,
        "total_actual_cost": total
    }


@app.post("/compare-to-budget")
async def compare_actual_to_budget(
    actual_cost: float,
    budgeted_cost: float
):
    """Compare actual cost to budget."""
    variance = budgeted_cost - actual_cost
    return {
        "actual_cost": actual_cost,
        "budgeted_cost": budgeted_cost,
        "variance": variance,
        "variance_percent": ((variance / budgeted_cost) * 100) if budgeted_cost != 0 else 0,
        "status": "Under budget" if variance > 0 else "Over budget" if variance < 0 else "On budget"
    }


@app.post("/variance-breakdown")
async def actual_cost_variance_breakdown(
    actual_material: float,
    budgeted_material: float,
    actual_labour: float,
    budgeted_labour: float,
    actual_overhead: float,
    budgeted_overhead: float
):
    """Break down variances by cost category."""
    material_variance = budgeted_material - actual_material
    labour_variance = budgeted_labour - actual_labour
    overhead_variance = budgeted_overhead - actual_overhead
    total_variance = material_variance + labour_variance + overhead_variance

    return {
        "material": {"actual": actual_material, "budgeted": budgeted_material, "variance": material_variance},
        "labour": {"actual": actual_labour, "budgeted": budgeted_labour, "variance": labour_variance},
        "overhead": {"actual": actual_overhead, "budgeted": budgeted_overhead, "variance": overhead_variance},
        "total_variance": total_variance
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
