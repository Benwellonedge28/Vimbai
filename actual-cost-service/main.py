"""
Vimbai Actual Cost Service
Handles actual cost calculations and comparisons.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "actual_cost_service" not in _sys.modules or not hasattr(_sys.modules.get("actual_cost_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("actual_cost_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["actual_cost_service"] = _pkg
    _sys.modules["actual_cost_service"].__path__ = [_HERE]

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List

import structlog
from actual_cost_service import calc
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "actual-cost-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8112"))

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Actual Cost Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.get("/health")
async def health_check():
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "status": "healthy",
        "calculation_engine": calc.engine_in_use(),
    }


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Actual cost calculations"}


@app.post("/material-actual")
async def calculate_actual_material_cost(actual_quantity: float, actual_price_per_unit: float):
    """Calculate actual material cost."""
    return calc.material(actual_quantity, actual_price_per_unit)


@app.post("/labour-actual")
async def calculate_actual_labour_cost(actual_hours: float, actual_rate_per_hour: float):
    """Calculate actual labour cost."""
    return calc.labour(actual_hours, actual_rate_per_hour)


@app.post("/total-actual")
async def calculate_total_actual_cost(
    actual_direct_material: float, actual_direct_labour: float, actual_overhead: float
):
    """Calculate total actual cost."""
    return calc.total(actual_direct_material, actual_direct_labour, actual_overhead)


@app.post("/compare-to-budget")
async def compare_actual_to_budget(actual_cost: float, budgeted_cost: float):
    """Compare actual cost to budget."""
    return calc.compare_to_budget(actual_cost, budgeted_cost)


@app.post("/variance-breakdown")
async def actual_cost_variance_breakdown(
    actual_material: float,
    budgeted_material: float,
    actual_labour: float,
    budgeted_labour: float,
    actual_overhead: float,
    budgeted_overhead: float,
):
    """Break down variances by cost category."""
    return calc.variance(
        actual_material,
        budgeted_material,
        actual_labour,
        budgeted_labour,
        actual_overhead,
        budgeted_overhead,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
