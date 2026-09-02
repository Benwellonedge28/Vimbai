"""
Vimbai Material Price Variance Service
Calculates material price variance.
Price Variance = (Standard Price - Actual Price) × Actual Quantity
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "material-price-variance-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8119"))

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

app = FastAPI(title="Vimbai Material Price Variance Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Material price variance calculation"}


@app.post("/calculate")
async def calculate_price_variance(standard_price: float, actual_price: float, actual_quantity: float):
    """
    Calculate Material Price Variance.
    Formula: (SP - AP) × AQ
    Favorable = SP > AP (paid less than standard)
    Adverse = SP < AP (paid more than standard)
    """
    variance = (standard_price - actual_price) * actual_quantity
    variance_type = "Favorable" if variance > 0 else "Adverse" if variance < 0 else "None"

    return {
        "standard_price": standard_price,
        "actual_price": actual_price,
        "actual_quantity": actual_quantity,
        "price_variance": round(variance, 2),
        "variance_type": variance_type,
        "formula": f"({standard_price} - {actual_price}) × {actual_quantity} = {variance}",
    }


@app.post("/standard-vs-actual-price")
async def standard_vs_actual_price(standard_price_per_kg: float, actual_price_per_kg: float, quantity_kg: float):
    """Calculate price variance with detailed breakdown."""
    variance = (standard_price_per_kg - actual_price_per_kg) * quantity_kg
    std_total = standard_price_per_kg * quantity_kg
    actual_total = actual_price_per_kg * quantity_kg

    return {
        "standard_price_per_kg": standard_price_per_kg,
        "actual_price_per_kg": actual_price_per_kg,
        "quantity_kg": quantity_kg,
        "standard_cost": std_total,
        "actual_cost": actual_total,
        "price_variance": round(variance, 2),
        "interpretation": "Paid less per unit" if variance > 0 else "Paid more per unit" if variance < 0 else "Same",
    }


@app.post("/total-price-variance")
async def calculate_total_price_variance(standard_price: float, actual_price: float, total_actual_quantity: float):
    """Calculate total price variance."""
    variance = (standard_price - actual_price) * total_actual_quantity
    return {
        "standard_price": standard_price,
        "actual_price": actual_price,
        "total_actual_quantity": total_actual_quantity,
        "total_price_variance": round(variance, 2),
        "type": "Favorable" if variance > 0 else "Adverse",
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
