"""
Vimbai Variance Service
Generic variance calculation (Budgeted - Actual).
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, Optional

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "variance-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8108"))

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

app = FastAPI(title="Vimbai Variance Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class VarianceResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    budgeted: float
    actual: float
    variance: float = 0
    variance_type: str = ""  # favorable or adverse
    percentage: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Generic variance calculation"}


@app.post("/calculate")
async def calculate_variance(budgeted: float, actual: float, is_cost: bool = True):
    """
    Calculate variance.
    Variance = Budgeted - Actual
    For costs: Favorable = Budgeted > Actual, Adverse = Budgeted < Actual
    For revenue: Favorable = Budgeted < Actual, Adverse = Budgeted > Actual
    """
    variance = budgeted - actual

    if is_cost:
        variance_type = "Favorable" if variance > 0 else "Adverse" if variance < 0 else "None"
    else:
        variance_type = "Favorable" if variance < 0 else "Adverse" if variance > 0 else "None"

    percentage = (variance / budgeted * 100) if budgeted != 0 else 0

    result = VarianceResult(
        budgeted=budgeted,
        actual=actual,
        variance=variance,
        variance_type=variance_type,
        percentage=round(percentage, 2),
    )
    return result


@app.post("/material-variance")
async def calculate_material_variance(
    standard_price: float, actual_price: float, standard_quantity: float, actual_quantity: float
):
    """
    Calculate material variances.
    Total Material Variance = (SP × SQ) - (AP × AQ)
    Price Variance = (SP - AP) × AQ
    Usage Variance = (SQ - AQ) × SP
    """
    standard_cost = standard_price * standard_quantity
    actual_cost = actual_price * actual_quantity
    total_variance = standard_cost - actual_cost

    price_variance = (standard_price - actual_price) * actual_quantity
    usage_variance = (standard_quantity - actual_quantity) * standard_price
    combined = price_variance + usage_variance

    return {
        "standard_price": standard_price,
        "actual_price": actual_price,
        "standard_quantity": standard_quantity,
        "actual_quantity": actual_quantity,
        "standard_cost": standard_cost,
        "actual_cost": actual_cost,
        "total_material_variance": round(total_variance, 2),
        "price_variance": round(price_variance, 2),
        "usage_variance": round(usage_variance, 2),
        "price_variance_type": "Favorable" if price_variance > 0 else "Adverse",
        "usage_variance_type": "Favorable" if usage_variance > 0 else "Adverse",
    }


@app.post("/labour-variance")
async def calculate_labour_variance(
    standard_rate: float, actual_rate: float, standard_hours: float, actual_hours: float
):
    """
    Calculate labour variances.
    Total Labour Variance = (SR × SH) - (AR × AH)
    Rate Variance = (SR - AR) × AH
    Efficiency Variance = (SH - AH) × SR
    """
    standard_cost = standard_rate * standard_hours
    actual_cost = actual_rate * actual_hours
    total_variance = standard_cost - actual_cost

    rate_variance = (standard_rate - actual_rate) * actual_hours
    efficiency_variance = (standard_hours - actual_hours) * standard_rate

    return {
        "standard_rate": standard_rate,
        "actual_rate": actual_rate,
        "standard_hours": standard_hours,
        "actual_hours": actual_hours,
        "standard_cost": standard_cost,
        "actual_cost": actual_cost,
        "total_labour_variance": round(total_variance, 2),
        "rate_variance": round(rate_variance, 2),
        "efficiency_variance": round(efficiency_variance, 2),
        "rate_variance_type": "Favorable" if rate_variance > 0 else "Adverse",
        "efficiency_variance_type": "Favorable" if efficiency_variance > 0 else "Adverse",
    }


@app.post("/sales-variance")
async def calculate_sales_variance(
    standard_price: float, actual_price: float, standard_quantity: float, actual_quantity: float
):
    """
    Calculate sales variances.
    Total Sales Variance = (SP × SQ) - (AP × AQ)
    Price Variance = (AP - SP) × AQ
    Volume Variance = (AQ - SQ) × SP
    """
    standard_revenue = standard_price * standard_quantity
    actual_revenue = actual_price * actual_quantity
    total_variance = actual_revenue - standard_revenue

    price_variance = (actual_price - standard_price) * actual_quantity
    volume_variance = (actual_quantity - standard_quantity) * standard_price

    return {
        "standard_price": standard_price,
        "actual_price": actual_price,
        "standard_quantity": standard_quantity,
        "actual_quantity": actual_quantity,
        "standard_revenue": standard_revenue,
        "actual_revenue": actual_revenue,
        "total_sales_variance": round(total_variance, 2),
        "price_variance": round(price_variance, 2),
        "volume_variance": round(volume_variance, 2),
        "price_variance_type": "Favorable" if price_variance > 0 else "Adverse",
        "volume_variance_type": "Favorable" if volume_variance > 0 else "Adverse",
    }


@app.post("/overhead-variance")
async def calculate_overhead_variance(
    standard_overhead: float, actual_overhead: float, budgeted_output: float, actual_output: float, overhead_rate: float
):
    """Calculate overhead variance."""
    # Volume variance = (Budgeted output - Actual output) × OAR
    volume_variance = (budgeted_output - actual_output) * overhead_rate

    # Spending variance = Budgeted overhead - Actual overhead
    spending_variance = standard_overhead - actual_overhead

    total_variance = volume_variance + spending_variance

    return {
        "standard_overhead": standard_overhead,
        "actual_overhead": actual_overhead,
        "budgeted_output": budgeted_output,
        "actual_output": actual_output,
        "overhead_rate": overhead_rate,
        "volume_variance": round(volume_variance, 2),
        "spending_variance": round(spending_variance, 2),
        "total_overhead_variance": round(total_variance, 2),
        "interpretation": "Favorable" if total_variance > 0 else "Adverse",
    }


@app.post("/flexible-budget-variance")
async def calculate_flexible_budget_variance(static_budget: float, flexed_budget: float, actual: float):
    """Calculate flexible budget variances."""
    # Sales volume variance = Flexed budget - Static budget
    sales_volume_var = flexed_budget - static_budget

    # Expenditure variance = Flexed budget - Actual
    expenditure_var = flexed_budget - actual

    # Total variance = Static budget - Actual
    total_var = static_budget - actual

    return {
        "static_budget": static_budget,
        "flexed_budget": flexed_budget,
        "actual": actual,
        "sales_volume_variance": round(sales_volume_var, 2),
        "expenditure_variance": round(expenditure_var, 2),
        "total_variance": round(total_var, 2),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
