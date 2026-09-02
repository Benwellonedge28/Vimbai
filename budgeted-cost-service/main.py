"""
Vimbai Budgeted Cost Service
Handles budgeted cost calculations.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import structlog
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


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
    total = direct_material + direct_labour + overhead + other_costs
    return {
        "department": department,
        "period": period,
        "direct_material": direct_material,
        "direct_labour": direct_labour,
        "overhead": overhead,
        "other_costs": other_costs,
        "total_budgeted_cost": total,
    }


@app.post("/flexible-budget")
async def calculate_flexible_budget(budgeted_cost_per_unit: float, actual_output: float, budgeted_output: float):
    """Calculate flexible budget variance."""
    flexed_budget = budgeted_cost_per_unit * actual_output
    original_budget = budgeted_cost_per_unit * budgeted_output

    return {
        "budgeted_cost_per_unit": budgeted_cost_per_unit,
        "budgeted_output": budgeted_output,
        "actual_output": actual_output,
        "original_budget": original_budget,
        "flexed_budget": flexed_budget,
        "volume_variance": flexed_budget - original_budget,
    }


@app.post("/total-budget")
async def calculate_total_budget(budgeted_costs: List[dict]):  # [{"item": "name", "amount": x}]
    """Calculate total budget from items."""
    items = []
    total = 0
    for item in budgeted_costs:
        amount = item.get("amount", 0)
        total += amount
        items.append({"item": item.get("item", ""), "amount": amount})

    return {"budget_items": items, "total_budgeted_cost": total}


@app.post("/compare-output")
async def compare_budgeted_to_actual_output(budgeted_units: float, actual_units: float, budgeted_cost_per_unit: float):
    """Compare budgeted vs actual output costs."""
    budgeted_total = budgeted_units * budgeted_cost_per_unit
    flexed_budget = actual_units * budgeted_cost_per_unit

    return {
        "budgeted_units": budgeted_units,
        "actual_units": actual_units,
        "budgeted_cost_per_unit": budgeted_cost_per_unit,
        "budgeted_total_cost": budgeted_total,
        "flexed_budget_cost": flexed_budget,
        "output_variance": budgeted_total - flexed_budget,
    }


@app.post("/cost-breakdown")
async def budgeted_cost_breakdown(
    total_budgeted_cost: float, material_percent: float, labour_percent: float, overhead_percent: float
):
    """Break down total budget into cost categories."""
    total_percent = material_percent + labour_percent + overhead_percent
    if abs(total_percent - 100) > 0.01:
        return {"error": "Percentages must sum to 100"}

    return {
        "total_budgeted_cost": total_budgeted_cost,
        "material": total_budgeted_cost * (material_percent / 100),
        "labour": total_budgeted_cost * (labour_percent / 100),
        "overhead": total_budgeted_cost * (overhead_percent / 100),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
