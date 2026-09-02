"""
Vimbai Trading Account Service
Manufacturing and trading account preparation for cost of goods sold and gross profit.
Port: 8337
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Dict, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "trading-account-service"
PORT = int(os.getenv("PORT", "8337"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Trading Account Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class TradingAccountRequest(BaseModel):
    company_id: str
    period: str
    opening_stock: float = 0
    purchases: float = 0
    carriage_inward: float = 0
    closing_stock: float = 0
    sales: float = 0
    sales_returns: float = 0
    direct_wages: float = 0
    factory_overhead: float = 0


class TradingAccount(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period: str
    net_sales: float
    cost_of_goods_available: float
    cost_of_goods_sold: float
    gross_profit: float
    gross_profit_pct: float
    opening_stock: float
    purchases: float
    direct_wages: float
    factory_overhead: float
    closing_stock: float


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/generate", response_model=TradingAccount)
async def generate_trading_account(req: TradingAccountRequest):
    net_sales = req.sales - req.sales_returns
    total_purchases = req.purchases + req.carriage_inward
    cost_of_goods_available = req.opening_stock + total_purchases + req.direct_wages + req.factory_overhead
    cogs = cost_of_goods_available - req.closing_stock
    gross_profit = net_sales - cogs
    gross_profit_pct = (gross_profit / net_sales * 100) if net_sales else 0

    return TradingAccount(
        company_id=req.company_id,
        period=req.period,
        net_sales=round(net_sales, 2),
        cost_of_goods_available=round(cost_of_goods_available, 2),
        cost_of_goods_sold=round(cogs, 2),
        gross_profit=round(gross_profit, 2),
        gross_profit_pct=round(gross_profit_pct, 2),
        opening_stock=req.opening_stock,
        purchases=total_purchases,
        direct_wages=req.direct_wages,
        factory_overhead=req.factory_overhead,
        closing_stock=req.closing_stock,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
