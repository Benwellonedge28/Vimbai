"""
FinAcc Contribution Per Unit Service
Calculates contribution per unit and contribution margin.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "contribution-per-unit-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8109"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Contribution Per Unit Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Contribution per unit calculations"}


@app.post("/calculate")
async def calculate_contribution_per_unit(selling_price: float, variable_cost: float):
    """Calculate contribution per unit."""
    contribution = selling_price - variable_cost
    margin = (contribution / selling_price * 100) if selling_price != 0 else 0

    return {
        "selling_price": selling_price,
        "variable_cost": variable_cost,
        "contribution_per_unit": contribution,
        "contribution_margin_percent": round(margin, 2)
    }


@app.post("/total-contribution")
async def calculate_total_contribution(
    selling_price: float,
    variable_cost: float,
    quantity: int
):
    """Calculate total contribution."""
    contribution_per_unit = selling_price - variable_cost
    total_contribution = contribution_per_unit * quantity

    return {
        "selling_price": selling_price,
        "variable_cost": variable_cost,
        "contribution_per_unit": contribution_per_unit,
        "quantity": quantity,
        "total_contribution": total_contribution
    }


@app.post("/contribution-ratio")
async def calculate_contribution_ratio(selling_price: float, variable_cost: float):
    """Calculate contribution margin ratio (CM ratio)."""
    contribution = selling_price - variable_cost
    cm_ratio = (contribution / selling_price) if selling_price != 0 else 0

    return {
        "selling_price": selling_price,
        "variable_cost": variable_cost,
        "contribution_per_unit": contribution,
        "contribution_margin_ratio": round(cm_ratio, 4),
        "contribution_margin_percent": round(cm_ratio * 100, 2)
    }


@app.post("/multi-product")
async def calculate_multi_product_contribution(
    products: List[dict]  # [{"selling_price": x, "variable_cost": y, "quantity": z}]
):
    """Calculate weighted average contribution margin."""
    total_sales = 0
    total_variable_cost = 0
    total_contribution = 0

    details = []
    for p in products:
        sp = p.get("selling_price", 0)
        vc = p.get("variable_cost", 0)
        qty = p.get("quantity", 0)
        contrib = (sp - vc) * qty
        sales = sp * qty

        total_sales += sales
        total_variable_cost += vc * qty
        total_contribution += contrib

        details.append({
            "selling_price": sp,
            "variable_cost": vc,
            "quantity": qty,
            "contribution": contrib
        })

    wacm = (total_contribution / total_sales) if total_sales != 0 else 0

    return {
        "product_details": details,
        "total_sales": total_sales,
        "total_variable_cost": total_variable_cost,
        "total_contribution": total_contribution,
        "weighted_average_contribution_margin": round(wacm, 4)
    }


@app.post("/required-sales")
async def calculate_required_sales_for_target_profit(
    fixed_costs: float,
    target_profit: float,
    selling_price: float,
    variable_cost: float
):
    """Calculate required sales to achieve target profit."""
    contribution_per_unit = selling_price - variable_cost
    units_required = (fixed_costs + target_profit) / contribution_per_unit if contribution_per_unit != 0 else 0
    sales_required = units_required * selling_price

    return {
        "fixed_costs": fixed_costs,
        "target_profit": target_profit,
        "selling_price": selling_price,
        "variable_cost": variable_cost,
        "contribution_per_unit": contribution_per_unit,
        "units_required": round(units_required, 2),
        "sales_required": round(sales_required, 2)
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
