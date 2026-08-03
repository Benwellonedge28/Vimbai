"""
Vimbai Sales Price Variance Service
Calculates sales price variance.
Price Variance = (Actual Price - Standard Price) × Actual Quantity
"""

import os
import uuid
from datetime import datetime
from typing:Any, Dict

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "sales-price-variance-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8125"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Sales Price Variance Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Sales price variance calculation"}


@app.post("/calculate")
async def calculate_price_variance(
    actual_price: float,
    standard_price: float,
    actual_quantity: float
):
    """
    Calculate Sales Price Variance.
    Formula: (AP - SP) × AQ
    Favorable = AP > SP (sold at higher price than standard)
    Adverse = AP < SP (sold at lower price than standard)
    """
    variance = (actual_price - standard_price) * actual_quantity
    variance_type = "Favorable" if variance > 0 else "Adverse" if variance < 0 else "None"

    return {
        "actual_price": actual_price,
        "standard_price": standard_price,
        "actual_quantity": actual_quantity,
        "price_variance": round(variance, 2),
        "variance_type": variance_type,
        "actual_revenue": actual_price * actual_quantity,
        "standard_revenue": standard_price * actual_quantity,
        "formula": f"({actual_price} - {standard_price}) × {actual_quantity} = {variance}"
    }


@app.post("/per-unit")
async def price_variance_per_unit(
    actual_price_per_unit: float,
    standard_price_per_unit: float,
    quantity_sold: float
):
    """Calculate price variance per unit."""
    variance = (actual_price_per_unit - standard_price_per_unit) * quantity_sold
    return {
        "actual_price_per_unit": actual_price_per_unit,
        "standard_price_per_unit": standard_price_per_unit,
        "quantity_sold": quantity_sold,
        "price_variance": round(variance, 2),
        "interpretation": "Higher selling price" if variance > 0 else "Lower selling price"
    }


@app.post("/total")
async def total_sales_price_variance(
    actual_prices: list,  # [{"price": x, "quantity": y}]
    standard_price: float
):
    """Calculate total price variance across multiple sales."""
    total_variance = 0
    details = []

    for sale in actual_prices:
        price = sale.get("price", 0)
        qty = sale.get("quantity", 0)
        var = (price - standard_price) * qty
        total_variance += var
        details.append({"price": price, "quantity": qty, "variance": round(var, 2)})

    return {
        "standard_price": standard_price,
        "sales_details": details,
        "total_price_variance": round(total_variance, 2),
        "type": "Favorable" if total_variance > 0 else "Adverse"
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
