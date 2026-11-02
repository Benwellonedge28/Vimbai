"""
Vimbai Standard Cost Service
Handles standard cost calculations.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "standard_cost_service" not in _sys.modules or not hasattr(_sys.modules.get("standard_cost_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("standard_cost_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["standard_cost_service"] = _pkg
    _sys.modules["standard_cost_service"].__path__ = [_HERE]

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field
from standard_cost_service import calc

SERVICE_NAME = "standard-cost-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8110"))

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

app = FastAPI(title="Vimbai Standard Cost Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Standard cost calculations"}


@app.post("/material-standard")
async def calculate_material_standard_cost(standard_price_per_unit: float, standard_quantity: float):
    """Calculate standard material cost."""
    return calc.material(standard_price_per_unit, standard_quantity)


@app.post("/labour-standard")
async def calculate_labour_standard_cost(standard_rate_per_hour: float, standard_hours: float):
    """Calculate standard labour cost."""
    return calc.labour(standard_rate_per_hour, standard_hours)


@app.post("/overhead-standard")
async def calculate_overhead_standard_cost(overhead_rate: float, standard_activity_level: float):
    """Calculate standard overhead cost."""
    return calc.overhead(overhead_rate, standard_activity_level)


@app.post("/total-standard")
async def calculate_total_standard_cost(
    standard_direct_material: float, standard_direct_labour: float, standard_overhead: float
):
    """Calculate total standard cost."""
    return calc.total(standard_direct_material, standard_direct_labour, standard_overhead)


@app.post("/per-unit")
async def calculate_standard_cost_per_unit(
    standard_material_cost: float, standard_labour_cost: float, standard_overhead: float, units_produced: float
):
    """Calculate standard cost per unit."""
    return calc.per_unit(standard_material_cost, standard_labour_cost, standard_overhead, units_produced)


@app.post("/variance-analysis")
async def standard_cost_variance_analysis(standard_cost: float, actual_cost: float):
    """Analyze variance between standard and actual cost."""
    return calc.variance(standard_cost, actual_cost)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
