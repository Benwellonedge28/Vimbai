"""
FinAcc Sales Volume Variance Service
Calculates sales volume variance.
Volume Variance = (Actual Quantity - Standard Quantity) × Standard Price
"""

import os
import uuid
from datetime import datetime
from typing:Any, Dict

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "sales-volume-variance-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8126"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Sales Volume Variance Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Sales volume variance calculation"}


@app.post("/calculate")
async def calculate_volume_variance(
    actual_quantity: float,
    standard_quantity: float,
    standard_price: float
):
    """
    Calculate Sales Volume Variance.
    Formula: (AQ - SQ) × SP
    Favorable = AQ > SQ (sold more than budgeted)
    Adverse = AQ < SQ (sold less than budgeted)
    """
    variance = (actual_quantity - standard_quantity) * standard_price
    quantity_diff = actual_quantity - standard_quantity
    variance_type = "Favorable" if variance > 0 else "Adverse" if variance < 0 else "None"

    return {
        "actual_quantity": actual_quantity,
        "standard_quantity": standard_quantity,
        "standard_price": standard_price,
        "quantity_difference": quantity_diff,
        "volume_variance": round(variance, 2),
        "variance_type": variance_type,
        "formula": f"({actual_quantity} - {standard_quantity}) × {standard_price} = {variance}"
    }


@app.post("/revenue-basis")
async def volume_variance_revenue_basis(
    actual_sales: float,
    budgeted_sales: float
):
    """Calculate volume variance based on sales revenue."""
    variance = actual_sales - budgeted_sales
    return {
        "actual_sales": actual_sales,
        "budgeted_sales": budgeted_sales,
        "volume_variance": round(variance, 2),
        "type": "Favorable" if variance > 0 else "Adverse"
    }


@app.post("/contribution-margin")
async def volume_variance_contribution(
    actual_quantity: float,
    standard_quantity: float,
    contribution_per_unit: float
):
    """Calculate volume variance using contribution margin."""
    variance = (actual_quantity - standard_quantity) * contribution_per_unit
    return {
        "actual_quantity": actual_quantity,
        "standard_quantity": standard_quantity,
        "contribution_per_unit": contribution_per_unit,
        "volume_variance": round(variance, 2),
        "type": "Favorable" if variance > 0 else "Adverse"
    }


@app.post("/total-sales-variance")
async def calculate_total_sales_variance(
    actual_price: float,
    standard_price: float,
    actual_quantity: float,
    standard_quantity: float
):
    """Calculate total sales variance (price + volume)."""
    actual_revenue = actual_price * actual_quantity
    standard_revenue = standard_price * standard_quantity
    total_variance = actual_revenue - standard_revenue

    # Price variance
    price_variance = (actual_price - standard_price) * actual_quantity

    # Volume variance
    volume_variance = (actual_quantity - standard_quantity) * standard_price

    return {
        "actual_revenue": actual_revenue,
        "standard_revenue": standard_revenue,
        "total_sales_variance": round(total_variance, 2),
        "price_variance": round(price_variance, 2),
        "volume_variance": round(volume_variance, 2),
        "variance_sum": round(price_variance + volume_variance, 2),
        "overall": "Favorable" if total_variance > 0 else "Adverse"
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
