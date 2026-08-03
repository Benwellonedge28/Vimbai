"""
Vimbai Material Usage Variance Service
Calculates material usage/volume variance.
Usage Variance = (Standard Quantity - Actual Quantity) × Standard Price
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "material-usage-variance-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8120"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Material Usage Variance Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Material usage variance calculation"}


@app.post("/calculate")
async def calculate_usage_variance(
    standard_quantity: float,
    actual_quantity: float,
    standard_price: float
):
    """
    Calculate Material Usage Variance.
    Formula: (SQ - AQ) × SP
    Favorable = SQ > AQ (used less material than standard)
    Adverse = SQ < AQ (used more material than standard)
    """
    variance = (standard_quantity - actual_quantity) * standard_price
    variance_type = "Favorable" if variance > 0 else "Adverse" if variance < 0 else "None"

    return {
        "standard_quantity": standard_quantity,
        "actual_quantity": actual_quantity,
        "standard_price": standard_price,
        "quantity_difference": actual_quantity - standard_quantity,
        "usage_variance": round(variance, 2),
        "variance_type": variance_type,
        "formula": f"({standard_quantity} - {actual_quantity}) × {standard_price} = {variance}"
    }


@app.post("/efficiency-variance")
async def calculate_material_efficiency(
    standard_quantity_per_unit: float,
    units_produced: float,
    actual_total_quantity: float,
    standard_price: float
):
    """Calculate material efficiency variance."""
    std_qty_total = standard_quantity_per_unit * units_produced
    variance = (std_qty_total - actual_total_quantity) * standard_price

    return {
        "standard_quantity_per_unit": standard_quantity_per_unit,
        "units_produced": units_produced,
        "standard_quantity_total": std_qty_total,
        "actual_quantity": actual_total_quantity,
        "standard_price": standard_price,
        "efficiency_variance": round(variance, 2),
        "type": "Favorable" if variance > 0 else "Adverse"
    }


@app.post("/total-material-variance")
async def calculate_total_material_variance(
    standard_price: float,
    actual_price: float,
    standard_quantity: float,
    actual_quantity: float
):
    """Calculate total material cost variance (price + usage)."""
    std_cost = standard_price * standard_quantity
    actual_cost = actual_price * actual_quantity
    total_variance = std_cost - actual_cost

    price_variance = (standard_price - actual_price) * actual_quantity
    usage_variance = (standard_quantity - actual_quantity) * standard_price

    return {
        "standard_price": standard_price,
        "actual_price": actual_price,
        "standard_quantity": standard_quantity,
        "actual_quantity": actual_quantity,
        "standard_cost": std_cost,
        "actual_cost": actual_cost,
        "total_material_variance": round(total_variance, 2),
        "price_variance": round(price_variance, 2),
        "usage_variance": round(usage_variance, 2),
        "reconciliation": f"{price_variance} + {usage_variance} = {round(price_variance + usage_variance, 2)}"
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
