"""
Vimbai Inventory Valuation Service
FIFO, LIFO, weighted average, and specific identification inventory costing.
Port: 8377
"""

import os
from datetime import datetime, timezone
from typing import Any, Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "inventory-valuation-service"
PORT = int(os.getenv("PORT", "8377"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Inventory Valuation Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class PurchaseRecord(BaseModel):
    date: str
    quantity: int
    unit_cost: float


class SaleRecord(BaseModel):
    date: str
    quantity: int
    unit_price: float


class InventoryValuationRequest(BaseModel):
    company_id: str
    method: str  # fifo, lifo, weighted_average, specific_identification
    purchases: List[PurchaseRecord]
    sales: List[SaleRecord]
    opening_inventory: float = 0
    opening_qty: int = 0


class InventoryValuationResponse(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()) if "uuid" in dir() else "n/a")
    company_id: str
    method: str
    total_goods_available: float
    total_units_available: int
    total_units_sold: int
    cost_of_goods_sold: float
    ending_inventory: float
    ending_qty: int
    total_sales_revenue: float
    gross_profit: float
    gross_margin: float


import uuid


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/calculate", response_model=InventoryValuationResponse)
async def calculate_inventory(req: InventoryValuationRequest):
    # Sort purchases by date
    sorted_purchases = sorted(req.purchases, key=lambda p: p.date)
    total_units = req.opening_qty + sum(p.quantity for p in sorted_purchases)
    total_cost = req.opening_inventory + sum(p.quantity * p.unit_cost for p in sorted_purchases)
    units_sold = sum(s.quantity for s in req.sales)
    sales_revenue = sum(s.quantity * s.unit_price for s in req.sales)

    if req.method == "fifo":
        remaining_units = units_sold
        cogs = 0
        remaining_cost = req.opening_inventory
        remaining_qty = req.opening_qty
        layers = [(p.date, p.quantity, p.unit_cost) for p in sorted_purchases]

        # Opening inventory first
        if remaining_units > 0 and remaining_qty > 0:
            taken = min(remaining_units, remaining_qty)
            cogs += remaining_cost * (taken / remaining_qty) if remaining_qty else 0
            remaining_units -= taken
            remaining_cost = remaining_cost * (1 - taken / remaining_qty) if remaining_qty else 0
            remaining_qty -= taken

        # Then purchases in order
        for date, qty, cost in layers:
            if remaining_units <= 0:
                break
            taken = min(remaining_units, qty)
            cogs += taken * cost
            remaining_units -= taken

        ending_qty = total_units - units_sold
        ending_inv = total_cost - cogs

    elif req.method == "lifo":
        remaining_units = units_sold
        cogs = 0
        layers = [(p.date, p.quantity, p.unit_cost) for p in sorted_purchases]

        # Last purchases first
        for date, qty, cost in reversed(layers):
            if remaining_units <= 0:
                break
            taken = min(remaining_units, qty)
            cogs += taken * cost
            remaining_units -= taken

        # Then opening inventory
        if remaining_units > 0 and remaining_qty > 0:
            taken = min(remaining_units, remaining_qty)
            cogs += taken * (req.opening_inventory / remaining_qty)

        ending_qty = total_units - units_sold
        ending_inv = total_cost - cogs

    else:  # weighted_average
        avg_cost = total_cost / total_units if total_units else 0
        cogs = avg_cost * units_sold
        ending_qty = total_units - units_sold
        ending_inv = avg_cost * ending_qty

    gross_profit = sales_revenue - cogs
    gross_margin = (gross_profit / sales_revenue * 100) if sales_revenue else 0

    return InventoryValuationResponse(
        company_id=req.company_id,
        method=req.method,
        total_goods_available=round(total_cost, 2),
        total_units_available=total_units,
        total_units_sold=units_sold,
        cost_of_goods_sold=round(cogs, 2),
        ending_inventory=round(ending_inv, 2),
        ending_qty=ending_qty,
        total_sales_revenue=round(sales_revenue, 2),
        gross_profit=round(gross_profit, 2),
        gross_margin=round(gross_margin, 2),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
