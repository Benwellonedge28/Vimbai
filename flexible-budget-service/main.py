"""
Vimbai Flexible Budget Service
Handles flexible budget calculations.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "flexible-budget-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8113"))

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

app = FastAPI(title="Vimbai Flexible Budget Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Flexible budget calculations"}


@app.post("/calculate")
async def calculate_flexible_budget(budgeted_cost: float, budgeted_activity: float, actual_activity: float):
    """Calculate flexed budget based on actual activity level."""
    if budgeted_activity == 0:
        return {"error": "Budgeted activity cannot be zero"}

    flexed_budget = (budgeted_cost / budgeted_activity) * actual_activity
    return {
        "budgeted_cost": budgeted_cost,
        "budgeted_activity": budgeted_activity,
        "actual_activity": actual_activity,
        "flexed_budget": flexed_budget,
        "budget_formula": f"({budgeted_cost} / {budgeted_activity}) × {actual_activity}",
    }


@app.post("/multi-level")
async def flexible_budget_multiple_levels(
    budgeted_fixed_cost: float, budgeted_variable_rate: float, budgeted_activity: float, actual_activity: float
):
    """Calculate flexible budget with fixed and variable components."""
    # Fixed cost remains constant
    flexed_fixed = budgeted_fixed_cost

    # Variable cost flexes with activity
    flexed_variable = budgeted_variable_rate * actual_activity

    # Total flexed budget
    flexed_total = flexed_fixed + flexed_variable

    # Original budget
    original_variable = budgeted_variable_rate * budgeted_activity
    original_total = budgeted_fixed_cost + original_variable

    # Variances
    spending_variance = original_total - flexed_total
    activity_variance = flexed_total - original_total

    return {
        "budgeted_fixed_cost": budgeted_fixed_cost,
        "budgeted_variable_rate": budgeted_variable_rate,
        "budgeted_activity": budgeted_activity,
        "actual_activity": actual_activity,
        "flexed_fixed_cost": flexed_fixed,
        "flexed_variable_cost": flexed_variable,
        "flexed_total_budget": flexed_total,
        "original_budget": original_total,
        "spending_variance": spending_variance,
        "activity_variance": activity_variance,
    }


@app.post("/variance-analysis")
async def flexible_budget_variance_analysis(static_budget: float, flexed_budget: float, actual_result: float):
    """Perform full flexible budget variance analysis."""
    # Sales volume variance = Flexed budget - Static budget
    sales_volume_variance = flexed_budget - static_budget

    # Expenditure variance = Flexed budget - Actual
    expenditure_variance = flexed_budget - actual_result

    # Total variance = Static budget - Actual
    total_variance = static_budget - actual_result

    return {
        "static_budget": static_budget,
        "flexed_budget": flexed_budget,
        "actual_result": actual_result,
        "sales_volume_variance": round(sales_volume_variance, 2),
        "expenditure_variance": round(expenditure_variance, 2),
        "total_variance": round(total_variance, 2),
        "interpretation": "Favorable" if total_variance > 0 else "Adverse" if total_variance < 0 else "None",
    }


@app.post("/cost-per-unit")
async def calculate_flexed_cost_per_unit(flexed_budget: float, actual_activity: float):
    """Calculate flexed cost per unit."""
    cost_per_unit = flexed_budget / actual_activity if actual_activity > 0 else 0
    return {
        "flexed_budget": flexed_budget,
        "actual_activity": actual_activity,
        "flexed_cost_per_unit": round(cost_per_unit, 2),
    }


@app.post("/prepare-flexed")
async def prepare_flexed_budget(
    original_budget: float, original_output: float, actual_output: float, is_cost: bool = True
):
    """Prepare flexed budget and analyze variances."""
    flexed_budget = (original_budget / original_output) * actual_output if original_output > 0 else 0

    if is_cost:
        volume_variance = flexed_budget - original_budget
        variance_type = "Adverse" if volume_variance > 0 else "Favorable" if volume_variance < 0 else "None"
    else:
        volume_variance = flexed_budget - original_budget
        variance_type = "Favorable" if volume_variance > 0 else "Adverse" if volume_variance < 0 else "None"

    return {
        "original_budget": original_budget,
        "original_output": original_output,
        "actual_output": actual_output,
        "flexed_budget": round(flexed_budget, 2),
        "volume_variance": round(volume_variance, 2),
        "variance_type": variance_type,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
