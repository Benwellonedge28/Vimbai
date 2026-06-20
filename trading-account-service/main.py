"""
FinAcc Trading Account Service
Generates trading account for manufacturing/merchandising businesses.
"""

import os
import uuid
from datetime import datetime
from typing:Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "trading-account-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8133"))
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Trading Account Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Trading account generation"}


@app.post("/generate")
async def generate_trading_account(
    opening_stock: float,
    purchases: float,
    carriage_inwards: float = 0,
    wages_direct: float = 0,
    manufacturing_overhead: float = 0,
    closing_stock: float = 0,
    sales: float = 0,
    sales_returns: float = 0
):
    """
    Generate Trading Account.

    For Manufacturing Business:
    Cost of Goods Manufactured = Opening WIP + Direct Costs + Manufacturing Overhead - Closing WIP

    For Trading Business:
    Cost of Goods Sold = Opening Stock + Purchases + Direct Expenses - Closing Stock
    """
    # Calculate Cost of Goods Sold
    direct_costs = purchases + carriage_inwards + wages_direct
    cost_of_goods_available = opening_stock + direct_costs + manufacturing_overhead
    cost_of_goods_sold = cost_of_goods_available - closing_stock

    # Calculate Net Sales
    net_sales = sales - sales_returns

    # Calculate Gross Profit
    gross_profit = net_sales - cost_of_goods_sold
    gross_profit_margin = (gross_profit / net_sales * 100) if net_sales != 0 else 0

    return {
        "trading_account": {
            "sales": sales,
            "sales_returns": sales_returns,
            "net_sales": net_sales
        },
        "cost_of_goods_sold": {
            "opening_stock": opening_stock,
            "purchases": purchases,
            "carriage_inwards": carriage_inwards,
            "wages_direct": wages_direct,
            "manufacturing_overhead": manufacturing_overhead,
            "cost_of_goods_available": cost_of_goods_available,
            "less_closing_stock": closing_stock,
            "cost_of_goods_sold": cost_of_goods_sold
        },
        "gross_profit": gross_profit,
        "gross_profit_margin_percent": round(gross_profit_margin, 2)
    }


@app.post("/manufacturing-account")
async def generate_manufacturing_account(
    raw_materials_opening: float,
    raw_materials_purchases: float,
    carriage_inwards: float = 0,
    raw_materials_closing: float = 0,
    direct_wages: float = 0,
    direct_expenses: float = 0,
    factory_overhead: float = 0,
    opening_work_in_progress: float = 0,
    closing_work_in_progress: float = 0
):
    """Generate Manufacturing Account."""
    # Cost of Raw Materials Consumed
    cost_of_raw_consumed = raw_materials_opening + raw_materials_purchases + carriage_inwards - raw_materials_closing

    # Prime Cost
    prime_cost = cost_of_raw_consumed + direct_wages + direct_expenses

    # Factory Cost
    factory_cost = prime_cost + factory_overhead + opening_work_in_progress - closing_work_in_progress

    # Cost of Goods Manufactured
    cost_of_goods_manufactured = factory_cost

    return {
        "raw_materials": {
            "opening_stock": raw_materials_opening,
            "purchases": raw_materials_purchases,
            "carriage_inwards": carriage_inwards,
            "available_for_use": raw_materials_opening + raw_materials_purchases + carriage_inwards,
            "less_closing_stock": raw_materials_closing,
            "cost_of_raw_consumed": cost_of_raw_consumed
        },
        "prime_cost": prime_cost,
        "factory_overhead": factory_overhead,
        "work_in_progress": {
            "opening": opening_work_in_progress,
            "closing": closing_work_in_progress,
            "add_back": opening_work_in_progress - closing_work_in_progress
        },
        "cost_of_goods_manufactured": cost_of_goods_manufactured
    }


@app.post("/combined")
async def combined_trading_and_manufacturing(
    # Manufacturing
    raw_materials_opening: float,
    raw_materials_purchases: float,
    carriage_inwards: float = 0,
    raw_materials_closing: float = 0,
    direct_wages: float = 0,
    direct_expenses: float = 0,
    factory_overhead: float = 0,
    opening_work_in_progress: float = 0,
    closing_work_in_progress: float = 0,
    # Trading
    sales: float,
    sales_returns: float = 0,
    opening_finished_goods: float = 0,
    closing_finished_goods: float = 0,
    carriage_outwards: float = 0
):
    """Generate combined Manufacturing and Trading Account."""
    # Manufacturing
    cost_of_raw_consumed = raw_materials_opening + raw_materials_purchases + carriage_inwards - raw_materials_closing
    prime_cost = cost_of_raw_consumed + direct_wages + direct_expenses
    factory_cost = prime_cost + factory_overhead + opening_work_in_progress - closing_work_in_progress
    cost_of_goods_manufactured = factory_cost

    # Trading
    cost_of_finished_goods_available = opening_finished_goods + cost_of_goods_manufactured
    cost_of_goods_sold = cost_of_finished_goods_available - closing_finished_goods
    net_sales = sales - sales_returns
    gross_profit = net_sales - cost_of_goods_sold

    return {
        "manufacturing_account": {
            "cost_of_raw_consumed": cost_of_raw_consumed,
            "prime_cost": prime_cost,
            "factory_cost": factory_cost,
            "cost_of_goods_manufactured": cost_of_goods_manufactured
        },
        "trading_account": {
            "sales": sales,
            "sales_returns": sales_returns,
            "net_sales": net_sales,
            "opening_finished_goods": opening_finished_goods,
            "cost_of_goods_manufactured": cost_of_goods_manufactured,
            "goods_available": cost_of_finished_goods_available,
            "less_closing_finished_goods": closing_finished_goods,
            "cost_of_goods_sold": cost_of_goods_sold,
            "gross_profit": gross_profit
        },
        "gross_profit_margin_percent": round((gross_profit / net_sales * 100) if net_sales != 0 else 0, 2)
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
