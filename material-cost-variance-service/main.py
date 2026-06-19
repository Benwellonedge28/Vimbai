"""
FinAcc Material Cost Variance Service
Calculates total material cost variance.
Total Material Variance = Standard Cost - Actual Cost
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "material-cost-variance-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8121"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Material Cost Variance Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Material cost variance calculation"}


@app.post("/calculate")
async def calculate_material_cost_variance(
    standard_price: float,
    actual_price: float,
    standard_quantity: float,
    actual_quantity: float
):
    """
    Calculate Total Material Cost Variance.
    Total = (SP × SQ) - (AP × AQ)
    Breakdown into Price and Usage variances.
    """
    standard_cost = standard_price * standard_quantity
    actual_cost = actual_price * actual_quantity
    total_variance = standard_cost - actual_cost

    # Price variance
    price_variance = (standard_price - actual_price) * actual_quantity

    # Usage variance
    usage_variance = (standard_quantity - actual_quantity) * standard_price

    return {
        "standard_price": standard_price,
        "actual_price": actual_price,
        "standard_quantity": standard_quantity,
        "actual_quantity": actual_quantity,
        "standard_cost": standard_cost,
        "actual_cost": actual_cost,
        "total_material_cost_variance": round(total_variance, 2),
        "price_variance": round(price_variance, 2),
        "usage_variance": round(usage_variance, 2),
        "total_check": round(price_variance + usage_variance, 2),
        "overall": "Favorable" if total_variance > 0 else "Adverse" if total_variance < 0 else "None"
    }


@app.post("/simple")
async def simple_material_variance(standard_cost: float, actual_cost: float):
    """Simple material variance calculation."""
    variance = standard_cost - actual_cost
    return {
        "standard_cost": standard_cost,
        "actual_cost": actual_cost,
        "variance": round(variance, 2),
        "type": "Favorable" if variance > 0 else "Adverse"
    }


@app.post("/multi-material")
async def multi_material_variance(materials: list):
    """Calculate variance for multiple materials."""
    total_std = 0
    total_actual = 0
    breakdown = []

    for m in materials:
        std = m.get("standard_price", 0) * m.get("standard_quantity", 0)
        act = m.get("actual_price", 0) * m.get("actual_quantity", 0)
        var = std - act
        total_std += std
        total_actual += act

        breakdown.append({
            "material": m.get("name", "Unknown"),
            "standard_cost": std,
            "actual_cost": act,
            "variance": round(var, 2)
        })

    total_variance = total_std - total_actual

    return {
        "materials": breakdown,
        "total_standard_cost": total_std,
        "total_actual_cost": total_actual,
        "total_variance": round(total_variance, 2)
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
