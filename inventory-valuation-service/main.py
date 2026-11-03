"""
Vimbai Inventory Valuation Service
FIFO, LIFO, weighted average, and specific identification inventory costing.
Port: 8377
"""

import importlib.util
import os
import sys
from datetime import datetime, timezone
from typing import Any, Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("inventory_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["inventory_calc"] = calc
_spec.loader.exec_module(calc)

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
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/calculate", response_model=InventoryValuationResponse)
async def calculate_inventory(req: InventoryValuationRequest):
    result = calc.inventory(
        req.company_id,
        req.method,
        [p.model_dump() for p in req.purchases],
        [s.model_dump() for s in req.sales],
        req.opening_inventory,
        req.opening_qty,
    )
    logger.info("Inventory valued", company=req.company_id, method=req.method)
    return InventoryValuationResponse(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
