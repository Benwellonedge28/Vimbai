"""
FinAcc Absorption Costing Statement Service
Generates trading account and profit statements using absorption costing.
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

SERVICE_NAME = "absorption-costing-statement-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8065"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Absorption Costing Statement Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class StatementLineItem(BaseModel):
    description: str
    amount: float
    is_total: bool = False
    is_subtotal: bool = False
    indent_level: int = 0


class TradingAccountStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period_start: datetime
    period_end: datetime
    line_items: List[StatementLineItem] = []

    # Trading Account Section
    opening_stock: float = 0
    purchases: float = 0
    carriage_inwards: float = 0
    closing_stock: float = 0
    cost_of_goods_sold: float = 0
    gross_profit: float = 0

    # Profit & Loss Section
    sales_revenue: float = 0
    distribution_costs: float = 0
    administrative_expenses: float = 0
    other_expenses: float = 0
    net_profit: float = 0

    status: str = "draft"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ProductionCostStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    period: str

    # Production Costs
    direct_materials_opening: float = 0
    direct_materials_purchases: float = 0
    direct_materials_closing: float = 0
    direct_materials_used: float = 0

    direct_labor: float = 0
    direct_expenses: float = 0
    prime_cost: float = 0

    factory_overhead: float = 0
    work_in_progress_opening: float = 0
    work_in_progress_closing: float = 0
    production_cost: float = 0

    units_produced: int = 0
    cost_per_unit: float = 0

    created_at: datetime = Field(default_factory=datetime.utcnow)


trading_statements: List[TradingAccountStatement] = []
production_statements: List[ProductionCostStatement] = []


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "POST":
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception:
        return {}


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Absorption costing statement generation"}


@app.post("/trading-account/generate")
async def generate_trading_account(
    company_id: str, period_start: datetime, period_end: datetime,
    opening_stock: float, purchases: float, carriage_inwards: float,
    closing_stock: float, sales_revenue: float
):
    """Generate trading account statement."""
    statement = TradingAccountStatement(
        company_id=company_id, period_start=period_start, period_end=period_end
    )

    statement.opening_stock = opening_stock
    statement.purchases = purchases
    statement.carriage_inwards = carriage_inwards
    statement.closing_stock = closing_stock
    statement.sales_revenue = sales_revenue

    # Calculate Cost of Goods Sold
    statement.cost_of_goods_sold = (
        opening_stock + purchases + carriage_inwards - closing_stock
    )

    # Calculate Gross Profit
    statement.gross_profit = sales_revenue - statement.cost_of_goods_sold

    # Build line items for trading section
    statement.line_items = [
        StatementLineItem(description="Sales Revenue", amount=sales_revenue, is_total=True),
        StatementLineItem(description="Opening Stock", amount=opening_stock, indent_level=1),
        StatementLineItem(description="Add: Purchases", amount=purchases, indent_level=1),
        StatementLineItem(description="Add: Carriage Inwards", amount=carriage_inwards, indent_level=1),
        StatementLineItem(description="Cost of Goods Available", amount=opening_stock + purchases + carriage_inwards, is_subtotal=True, indent_level=1),
        StatementLineItem(description="Less: Closing Stock", amount=closing_stock, indent_level=1),
        StatementLineItem(description="Cost of Goods Sold", amount=statement.cost_of_goods_sold, is_total=True),
        StatementLineItem(description="Gross Profit", amount=statement.gross_profit, is_total=True),
    ]

    trading_statements.append(statement)
    return statement


@app.post("/trading-account/{statement_id}/add-expenses")
async def add_expenses_to_statement(
    statement_id: str,
    distribution_costs: float = 0,
    administrative_expenses: float = 0,
    other_expenses: float = 0
):
    """Add expenses to complete profit statement."""
    statement = next((s for s in trading_statements if s.id == statement_id), None)
    if not statement:
        return {"error": "Statement not found"}

    statement.distribution_costs = distribution_costs
    statement.administrative_expenses = administrative_expenses
    statement.other_expenses = other_expenses

    # Calculate Net Profit
    total_expenses = distribution_costs + administrative_expenses + other_expenses
    statement.net_profit = statement.gross_profit - total_expenses

    # Add P&L line items
    statement.line_items.extend([
        StatementLineItem(description="Gross Profit", amount=statement.gross_profit, is_total=True),
        StatementLineItem(description="Less: Distribution Costs", amount=distribution_costs, indent_level=1),
        StatementLineItem(description="Less: Administrative Expenses", amount=administrative_expenses, indent_level=1),
        StatementLineItem(description="Less: Other Expenses", amount=other_expenses, indent_level=1),
        StatementLineItem(description="Net Profit", amount=statement.net_profit, is_total=True),
    ])

    statement.status = "completed"
    return statement


@app.post("/production-cost/generate")
async def generate_production_cost_statement(
    product_id: str, period: str,
    direct_materials_opening: float, direct_materials_purchases: float,
    direct_materials_closing: float, direct_labor: float, direct_expenses: float,
    factory_overhead: float, work_in_progress_opening: float,
    work_in_progress_closing: float, units_produced: int
):
    """Generate production cost statement."""
    statement = ProductionCostStatement(
        product_id=product_id, period=period
    )

    statement.direct_materials_opening = direct_materials_opening
    statement.direct_materials_purchases = direct_materials_purchases
    statement.direct_materials_closing = direct_materials_closing
    statement.direct_labor = direct_labor
    statement.direct_expenses = direct_expenses
    statement.factory_overhead = factory_overhead
    statement.work_in_progress_opening = work_in_progress_opening
    statement.work_in_progress_closing = work_in_progress_closing
    statement.units_produced = units_produced

    # Calculate Direct Materials Used
    statement.direct_materials_used = (
        direct_materials_opening + direct_materials_purchases - direct_materials_closing
    )

    # Calculate Prime Cost
    statement.prime_cost = (
        statement.direct_materials_used + direct_labor + direct_expenses
    )

    # Calculate Production Cost
    statement.production_cost = (
        statement.prime_cost + factory_overhead +
        work_in_progress_opening - work_in_progress_closing
    )

    # Calculate Cost Per Unit
    if units_produced > 0:
        statement.cost_per_unit = statement.production_cost / units_produced

    production_statements.append(statement)
    return statement


@app.get("/trading-account")
async def list_trading_statements(
    company_id: Optional[str] = None,
    period_start: Optional[datetime] = None,
    period_end: Optional[datetime] = None
):
    """List trading account statements."""
    result = trading_statements
    if company_id:
        result = [s for s in result if s.company_id == company_id]
    return {"statements": result}


@app.get("/production-cost")
async def list_production_statements(
    product_id: Optional[str] = None,
    period: Optional[str] = None
):
    """List production cost statements."""
    result = production_statements
    if product_id:
        result = [s for s in result if s.product_id == product_id]
    if period:
        result = [s for s in result if s.period == period]
    return {"statements": result}


@app.get("/trading-account/{statement_id}")
async def get_trading_statement(statement_id: str):
    """Get trading account statement details."""
    statement = next((s for s in trading_statements if s.id == statement_id), None)
    if not statement:
        return {"error": "Statement not found"}
    return statement


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)