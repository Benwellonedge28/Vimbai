"""Vimbai Absorption Costing Statement Service. Port: 8065.

Trading account and production cost statements. The two module-level
lists (trading_statements, production_statements) were process-global
and shared across ALL callers; they now persist to Neo4j as caller-
owned, Book-scoped records (X-User-Id / X-Book-ID). add-expenses
mutates the caller's own statement (delete-then-recreate upsert).

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "absorption_costing_statement_service" not in _sys.modules or not hasattr(
    _sys.modules.get("absorption_costing_statement_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "absorption_costing_statement_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["absorption_costing_statement_service"] = _pkg
    _sys.modules["absorption_costing_statement_service"].__path__ = [_HERE]

import os
import uuid
from datetime import datetime
from typing import List, Optional

import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from neo4j import AsyncSession

from absorption_costing_statement_service import crud
from absorption_costing_statement_service.dependencies import book_id_var, get_db_session, get_user_id
from absorption_costing_statement_service.exceptions import AbsorptionCostingStatementError
from absorption_costing_statement_service.models import (
    ProductionCostStatement,
    StatementLineItem,
    TradingAccountStatement,
)

SERVICE_NAME = "absorption-costing-statement-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8065"))


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

app = FastAPI(title="Vimbai Absorption Costing Statement Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(AbsorptionCostingStatementError)
async def _acst_error(request: Request, exc: AbsorptionCostingStatementError):
    return JSONResponse(
        status_code=getattr(exc, "status_code", 400), content={"detail": str(exc), "error": exc.__class__.__name__}
    )


@app.post("/trading-account/generate")
async def generate_trading_account(
    company_id: str,
    period_start: datetime,
    period_end: datetime,
    opening_stock: float,
    purchases: float,
    carriage_inwards: float,
    closing_stock: float,
    sales_revenue: float,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Generate trading account statement."""
    statement = TradingAccountStatement(company_id=company_id, period_start=period_start, period_end=period_end)

    statement.opening_stock = opening_stock
    statement.purchases = purchases
    statement.carriage_inwards = carriage_inwards
    statement.closing_stock = closing_stock
    statement.sales_revenue = sales_revenue

    # Calculate Cost of Goods Sold
    statement.cost_of_goods_sold = opening_stock + purchases + carriage_inwards - closing_stock

    # Calculate Gross Profit
    statement.gross_profit = sales_revenue - statement.cost_of_goods_sold

    # Build line items for trading section
    statement.line_items = [
        StatementLineItem(description="Sales Revenue", amount=sales_revenue, is_total=True),
        StatementLineItem(description="Opening Stock", amount=opening_stock, indent_level=1),
        StatementLineItem(description="Add: Purchases", amount=purchases, indent_level=1),
        StatementLineItem(description="Add: Carriage Inwards", amount=carriage_inwards, indent_level=1),
        StatementLineItem(
            description="Cost of Goods Available",
            amount=opening_stock + purchases + carriage_inwards,
            is_subtotal=True,
            indent_level=1,
        ),
        StatementLineItem(description="Less: Closing Stock", amount=closing_stock, indent_level=1),
        StatementLineItem(description="Cost of Goods Sold", amount=statement.cost_of_goods_sold, is_total=True),
        StatementLineItem(description="Gross Profit", amount=statement.gross_profit, is_total=True),
    ]

    await crud.create(db_session, caller_id, statement)
    return statement


@app.post("/trading-account/{statement_id}/add-expenses")
async def add_expenses_to_statement(
    statement_id: str,
    distribution_costs: float = 0,
    administrative_expenses: float = 0,
    other_expenses: float = 0,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add expenses to complete the caller's profit statement."""
    statements = await crud.list_all(db_session, caller_id, TradingAccountStatement)
    statement = next((s for s in statements if s.id == statement_id), None)
    if not statement:
        return {"error": "Statement not found"}

    statement.distribution_costs = distribution_costs
    statement.administrative_expenses = administrative_expenses
    statement.other_expenses = other_expenses

    # Calculate Net Profit
    total_expenses = distribution_costs + administrative_expenses + other_expenses
    statement.net_profit = statement.gross_profit - total_expenses

    # Add P&L line items
    statement.line_items.extend(
        [
            StatementLineItem(description="Gross Profit", amount=statement.gross_profit, is_total=True),
            StatementLineItem(description="Less: Distribution Costs", amount=distribution_costs, indent_level=1),
            StatementLineItem(
                description="Less: Administrative Expenses", amount=administrative_expenses, indent_level=1
            ),
            StatementLineItem(description="Less: Other Expenses", amount=other_expenses, indent_level=1),
            StatementLineItem(description="Net Profit", amount=statement.net_profit, is_total=True),
        ]
    )

    statement.status = "completed"
    # upsert the mutated statement
    await crud.delete_where(db_session, caller_id, TradingAccountStatement, {"id": statement_id})
    await crud.create(db_session, caller_id, statement)
    return statement


@app.post("/production-cost/generate")
async def generate_production_cost_statement(
    product_id: str,
    period: str,
    direct_materials_opening: float,
    direct_materials_purchases: float,
    direct_materials_closing: float,
    direct_labor: float,
    direct_expenses: float,
    factory_overhead: float,
    work_in_progress_opening: float,
    work_in_progress_closing: float,
    units_produced: int,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Generate production cost statement."""
    statement = ProductionCostStatement(product_id=product_id, period=period)

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
    statement.direct_materials_used = direct_materials_opening + direct_materials_purchases - direct_materials_closing

    # Calculate Prime Cost
    statement.prime_cost = statement.direct_materials_used + direct_labor + direct_expenses

    # Calculate Production Cost
    statement.production_cost = (
        statement.prime_cost + factory_overhead + work_in_progress_opening - work_in_progress_closing
    )

    # Calculate Cost Per Unit
    if units_produced > 0:
        statement.cost_per_unit = statement.production_cost / units_produced

    await crud.create(db_session, caller_id, statement)
    return statement


@app.get("/trading-account")
async def list_trading_statements(
    company_id: Optional[str] = None,
    period_start: Optional[datetime] = None,
    period_end: Optional[datetime] = None,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's trading account statements."""
    result = await crud.list_all(db_session, caller_id, TradingAccountStatement)
    if company_id:
        result = [s for s in result if s.company_id == company_id]
    return {"statements": result}


@app.get("/production-cost")
async def list_production_statements(
    product_id: Optional[str] = None,
    period: Optional[str] = None,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's production cost statements."""
    result = await crud.list_all(db_session, caller_id, ProductionCostStatement)
    if product_id:
        result = [s for s in result if s.product_id == product_id]
    if period:
        result = [s for s in result if s.period == period]
    return {"statements": result}


@app.get("/trading-account/{statement_id}")
async def get_trading_statement(
    statement_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's trading account statement details."""
    statement = await crud.find(db_session, caller_id, TradingAccountStatement, statement_id)
    if not statement:
        return {"error": "Statement not found"}
    return statement


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
