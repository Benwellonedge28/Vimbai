"""
Vimbai Budgeted Cost Service
Handles budgeted cost calculations.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "budgeted_cost_service" not in _sys.modules or not hasattr(_sys.modules.get("budgeted_cost_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("budgeted_cost_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["budgeted_cost_service"] = _pkg
    _sys.modules["budgeted_cost_service"].__path__ = [_HERE]

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import structlog
from budgeted_cost_service import calc
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "budgeted-cost-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8111"))

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

app = FastAPI(title="Vimbai Budgeted Cost Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Budgeted cost calculations"}


@app.post("/create")
async def create_budgeted_cost(
    department: str,
    period: str,
    direct_material: float = 0,
    direct_labour: float = 0,
    overhead: float = 0,
    other_costs: float = 0,
):
    """Create budgeted cost breakdown."""
    return calc.create(department, period, direct_material, direct_labour, overhead, other_costs)


@app.post("/flexible-budget")
async def calculate_flexible_budget(budgeted_cost_per_unit: float, actual_output: float, budgeted_output: float):
    """Calculate flexible budget variance."""
    return calc.flexible(budgeted_cost_per_unit, actual_output, budgeted_output)


@app.post("/total-budget")
async def calculate_total_budget(budgeted_costs: List[dict]):  # [{"item": "name", "amount": x}]
    """Calculate total budget from items."""
    return calc.total(budgeted_costs)


@app.post("/compare-output")
async def compare_budgeted_to_actual_output(budgeted_units: float, actual_units: float, budgeted_cost_per_unit: float):
    """Compare budgeted vs actual output costs."""
    return calc.compare_output(budgeted_units, actual_units, budgeted_cost_per_unit)


@app.post("/cost-breakdown")
async def budgeted_cost_breakdown(
    total_budgeted_cost: float, material_percent: float, labour_percent: float, overhead_percent: float
):
    """Break down total budget into cost categories."""
    return calc.breakdown(total_budgeted_cost, material_percent, labour_percent, overhead_percent)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
