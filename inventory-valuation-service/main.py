"""
FinAcc Inventory Valuation Service
Handles FIFO, LIFO, and Weighted Average inventory valuation methods.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "inventory-valuation-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8131"))
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Inventory Valuation Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class InventoryTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    date: str
    type: str  # purchase, sale
    quantity: float
    unit_cost: float
    total: float = 0


async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    """Call another internal FinAcc service."""
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            if data:
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Inventory valuation methods"}


@app.post("/fifo")
async def calculate_fifo_valuation(transactions: List[dict]):
    """
    FIFO (First In, First Out) - First purchased items sold first.
    Assumes oldest inventory is sold first.
    """
    purchases = []
    sales = []

    for t in transactions:
        t["total"] = t["quantity"] * t["unit_cost"]
        if t["type"] == "purchase":
            purchases.append(t)
        elif t["type"] == "sale":
            sales.append(t)

    purchases.sort(key=lambda x: x["date"])
    sales.sort(key=lambda x: x["date"])

    closing_stock = []
    remaining_purchases = purchases.copy()

    for sale in sales:
        qty_to_allocate = sale["quantity"]
        sale_cost = 0

        while qty_to_allocate > 0 and remaining_purchases:
            p = remaining_purchases[0]
            if p["quantity"] <= qty_to_allocate:
                sale_cost += p["quantity"] * p["unit_cost"]
                qty_to_allocate -= p["quantity"]
                remaining_purchases.pop(0)
            else:
                sale_cost += qty_to_allocate * p["unit_cost"]
                p["quantity"] -= qty_to_allocate
                qty_to_allocate = 0

        sale["cost_allocated"] = sale_cost

    for p in remaining_purchases:
        closing_stock.append(p)

    total_closing_value = sum(s["quantity"] * s["unit_cost"] for s in closing_stock)
    total_cost_of_sales = sum(s.get("cost_allocated", 0) for s in sales)

    return {
        "method": "FIFO",
        "closing_stock": closing_stock,
        "total_closing_value": round(total_closing_value, 2),
        "cost_of_goods_sold": round(total_cost_of_sales, 2),
        "interpretation": "Latest costs in closing stock, earliest costs in COGS"
    }


@app.post("/lifo")
async def calculate_lifo_valuation(transactions: List[dict]):
    """
    LIFO (Last In, First Out) - Last purchased items sold first.
    Assumes newest inventory is sold first.
    """
    purchases = []
    sales = []

    for t in transactions:
        t["total"] = t["quantity"] * t["unit_cost"]
        if t["type"] == "purchase":
            purchases.append(t)
        elif t["type"] == "sale":
            sales.append(t)

    purchases.sort(key=lambda x: x["date"], reverse=True)
    sales.sort(key=lambda x: x["date"])

    closing_stock = []
    remaining_purchases = purchases.copy()
    remaining_purchases.reverse()

    for sale in sales:
        qty_to_allocate = sale["quantity"]
        sale_cost = 0

        while qty_to_allocate > 0 and remaining_purchases:
            p = remaining_purchases[0]
            if p["quantity"] <= qty_to_allocate:
                sale_cost += p["quantity"] * p["unit_cost"]
                qty_to_allocate -= p["quantity"]
                remaining_purchases.pop(0)
            else:
                sale_cost += qty_to_allocate * p["unit_cost"]
                p["quantity"] -= qty_to_allocate
                qty_to_allocate = 0

        sale["cost_allocated"] = sale_cost

    remaining_purchases.reverse()
    for p in remaining_purchases:
        if p["quantity"] > 0:
            closing_stock.append(p)

    total_closing_value = sum(s["quantity"] * s["unit_cost"] for s in closing_stock)
    total_cost_of_sales = sum(s.get("cost_allocated", 0) for s in sales)

    return {
        "method": "LIFO",
        "closing_stock": closing_stock,
        "total_closing_value": round(total_closing_value, 2),
        "cost_of_goods_sold": round(total_cost_of_sales, 2),
        "interpretation": "Earliest costs in closing stock, latest costs in COGS"
    }


@app.post("/weighted-average")
async def calculate_weighted_average(transactions: List[dict]):
    """
    Weighted Average - Average cost of all purchases.
    """
    purchases = []
    sales = []

    for t in transactions:
        t["total"] = t["quantity"] * t["unit_cost"]
        if t["type"] == "purchase":
            purchases.append(t)
        elif t["type"] == "sale":
            sales.append(t)

    running_qty = 0
    running_value = 0
    closing_stock = []
    sales_details = []

    for t in transactions:
        if t["type"] == "purchase":
            running_qty += t["quantity"]
            running_value += t["quantity"] * t["unit_cost"]
        elif t["type"] == "sale":
            if running_qty > 0:
                avg_cost = running_value / running_qty
                sale_cost = t["quantity"] * avg_cost
                running_qty -= t["quantity"]
                running_value -= sale_cost
                sales_details.append({
                    **t,
                    "average_cost": avg_cost,
                    "cost_allocated": sale_cost
                })

    if running_qty > 0:
        closing_stock.append({
            "quantity": running_qty,
            "unit_cost": running_value / running_qty,
            "total": running_value
        })

    total_closing_value = running_value
    total_cost_of_sales = sum(s.get("cost_allocated", 0) for s in sales_details)

    return {
        "method": "Weighted Average",
        "closing_stock": closing_stock,
        "total_closing_value": round(total_closing_value, 2),
        "cost_of_goods_sold": round(total_cost_of_sales, 2),
        "interpretation": "Smooths out price fluctuations"
    }


@app.post("/compare-methods")
async def compare_inventory_methods(transactions: List[dict]):
    """Compare all three inventory valuation methods."""
    fifo_result = await calculate_fifo_valuation(transactions)
    lifo_result = await calculate_lifo_valuation(transactions)
    wa_result = await calculate_weighted_average(transactions)

    return {
        "fifo": {
            "closing_value": fifo_result["total_closing_value"],
            "cogs": fifo_result["cost_of_goods_sold"]
        },
        "lifo": {
            "closing_value": lifo_result["total_closing_value"],
            "cogs": lifo_result["cost_of_goods_sold"]
        },
        "weighted_average": {
            "closing_value": wa_result["total_closing_value"],
            "cogs": wa_result["cost_of_goods_sold"]
        },
        "recommendation": "FIFO best for stable prices; LIFO for inflation; WA for simplicity"
    }


@app.post("/specific-identification")
async def specific_identification(
    items: List[dict]  # [{"item_id": "x", "quantity": n, "unit_cost": c}]
):
    """Specific Identification - Each item tracked individually."""
    total_value = sum(item["quantity"] * item["unit_cost"] for item in items)
    return {
        "items": items,
        "total_inventory_value": round(total_value, 2),
        "method": "Specific Identification",
        "interpretation": "Most accurate but requires item-level tracking"
    }


@app.post("/cost-or-market")
async def cost_or_market(closing_stock_value: float, market_value: float):
    """Lower of Cost or Market valuation."""
    lower_value = min(closing_stock_value, market_value)
    write_down = closing_stock_value - lower_value

    return {
        "closing_stock_value": closing_stock_value,
        "market_value": market_value,
        "lower_of_cost_or_market": lower_value,
        "inventory_write_down": round(write_down, 2) if write_down > 0 else 0,
        "action": "Write down inventory" if write_down > 0 else "No write down required"
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
