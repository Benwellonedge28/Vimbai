"""
Vimbai Trading Account Service
Calculates trading account: gross profit, cost of sales, and net trading margins.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "trading-account-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8000"))

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

app = FastAPI(title="Vimbai Trading Account Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class TradingAccountRequest(BaseModel):
    company_id: str = ""
    period: str = ""
    opening_stock: float = 0.0
    purchases: float = 0.0
    carriage_inward: float = 0.0
    closing_stock: float = 0.0
    sales: float = 0.0
    sales_returns: float = 0.0
    direct_wages: float = 0.0
    factory_overhead: float = 0.0


class TradingAccountResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str = ""
    period: str = ""
    net_sales: float = 0.0
    cost_of_goods_sold: float = 0.0
    gross_profit: float = 0.0
    gross_profit_pct: float = 0.0
    details: Dict[str, float] = {}
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


trading_accounts: List[TradingAccountResult] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/generate", response_model=TradingAccountResult)
async def generate_trading_account(request: TradingAccountRequest):
    """Generate a trading account from the given inputs."""
    net_sales = request.sales - request.sales_returns
    cost_of_goods_sold = (
        request.opening_stock
        + request.purchases
        + request.carriage_inward
        + request.direct_wages
        + request.factory_overhead
        - request.closing_stock
    )
    gross_profit = net_sales - cost_of_goods_sold
    gross_profit_pct = (gross_profit / net_sales * 100) if net_sales else 0.0

    result = TradingAccountResult(
        company_id=request.company_id,
        period=request.period,
        net_sales=net_sales,
        cost_of_goods_sold=cost_of_goods_sold,
        gross_profit=gross_profit,
        gross_profit_pct=round(gross_profit_pct, 2),
        details={
            "opening_stock": request.opening_stock,
            "purchases": request.purchases,
            "carriage_inward": request.carriage_inward,
            "closing_stock": request.closing_stock,
            "sales": request.sales,
            "sales_returns": request.sales_returns,
            "direct_wages": request.direct_wages,
            "factory_overhead": request.factory_overhead,
        },
    )
    trading_accounts.append(result)
    logger.info("Trading account generated", account_id=result.id, period=request.period, gp=gross_profit)
    return result


@app.get("/accounts", response_model=List[TradingAccountResult])
async def list_accounts(period: Optional[str] = None):
    """List trading accounts."""
    if period:
        return [a for a in trading_accounts if a.period == period]
    return trading_accounts


@app.get("/accounts/{account_id}", response_model=TradingAccountResult)
async def get_account(account_id: str):
    """Get a specific trading account."""
    acct = next((a for a in trading_accounts if a.id == account_id), None)
    if not acct:
        raise HTTPException(status_code=404, detail="Trading account not found")
    return acct


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
