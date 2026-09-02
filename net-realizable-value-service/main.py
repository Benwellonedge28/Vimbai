"""
Vimbai Net Realizable Value (NRV) / Scrap Value Service
Handles NRV and scrap value calculations.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, Optional

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "net-realizable-value-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8107"))

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

app = FastAPI(title="Vimbai Net Realizable Value Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "description": "Net realizable value and scrap value calculations",
    }


@app.post("/calculate-nrv")
async def calculate_nrv(selling_Price: float, Completion_Cost: float, selling_cost: float = 0):
    """
    Calculate Net Realizable Value.
    NRV = Selling Price - Completion Cost - Selling Cost
    """
    nrv = selling_Price - Completion_Cost - selling_cost
    return {
        "selling_price": selling_Price,
        "completion_cost": Completion_Cost,
        "selling_cost": selling_cost,
        "net_realizable_value": nrv,
        "formula": f"{selling_Price} - {Completion_Cost} - {selling_cost} = {nrv}",
    }


@app.post("/scrap-value")
async def calculate_scrap_value(
    original_cost: float, accumulated_depreciation: float, disposal_cost: float = 0, proceeds_from_sale: float = 0
):
    """Calculate scrap value/net book value."""
    net_book_value = original_cost - accumulated_depreciation
    scrap_value = proceeds_from_sale - disposal_cost if proceeds_from_sale > 0 else 0

    return {
        "original_cost": original_cost,
        "accumulated_depreciation": accumulated_depreciation,
        "net_book_value": net_book_value,
        "disposal_cost": disposal_cost,
        "proceeds_from_sale": proceeds_from_sale,
        "scrap_value": scrap_value,
        "gain_loss_on_disposal": scrap_value - net_book_value,
    }


@app.post("/residual-value")
async def calculate_residual_value(original_cost: float, depreciation_rate: float, useful_life_years: int):
    """Calculate residual value using straight-line depreciation."""
    total_depreciation = original_cost * depreciation_rate * useful_life_years
    residual_value = original_cost - total_depreciation

    return {
        "original_cost": original_cost,
        "depreciation_rate": depreciation_rate,
        "useful_life_years": useful_life_years,
        "total_depreciation": total_depreciation,
        "residual_value": max(residual_value, 0),
    }


@app.post("/NRV-inventory")
async def calculate_nrv_for_inventory(
    estimated_selling_price: float, estimated_completion_cost: float, estimated_selling_expenses: float
):
    """Calculate NRV for inventory valuation (lower of cost or NRV)."""
    nrv = estimated_selling_price - estimated_completion_cost - estimated_selling_expenses

    return {
        "estimated_selling_price": estimated_selling_price,
        "estimated_completion_cost": estimated_completion_cost,
        "estimated_selling_expenses": estimated_selling_expenses,
        "net_realizable_value": nrv,
        "accounting_treatment": "Write down inventory to NRV if NRV < Cost",
    }


@app.post("/compare-cost-nrv")
async def compare_cost_and_nrv(cost: float, selling_price: float, completion_cost: float, selling_cost: float):
    """Compare cost with NRV for inventory valuation."""
    nrv = selling_price - completion_cost - selling_cost
    lower_value = min(cost, nrv) if nrv > 0 else cost

    return {
        "cost": cost,
        "selling_price": selling_price,
        "completion_cost": completion_cost,
        "selling_cost": selling_cost,
        "net_realizable_value": nrv,
        "lower_of_cost_or_nrv": lower_value,
        "write_down_required": cost > nrv,
        "write_down_amount": max(cost - nrv, 0),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
