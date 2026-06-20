"""
FinAcc Profit and Loss Account Service
Generates Income Statement (Profit and Loss Account).
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

SERVICE_NAME = "profit-loss-account-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8134"))
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Profit and Loss Account Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Profit and Loss Account"}


@app.post("/generate")
async def generate_profit_loss_account(
    gross_profit: float,
    other_incomes: List[dict] = [],  # [{"name": "Interest Received", "amount": x}]
    distribution_costs: float = 0,  # Selling & Distribution expenses
    administrative_expenses: float = 0,  # Office & Admin expenses
    financial_expenses: float = 0,  # Interest paid
    other_expenses: List[dict] = []  # [{"name": "Discount Allowed", "amount": x}]
):
    """
    Generate Profit and Loss Account.

    Net Profit = Gross Profit + Other Income - All Expenses
    """
    # Calculate other income
    total_other_income = sum(inc.get("amount", 0) for inc in other_incomes)

    # Calculate total expenses
    total_distribution = distribution_costs
    total_admin = administrative_expenses
    total_financial = financial_expenses
    total_other_expenses = sum(exp.get("amount", 0) for exp in other_expenses)
    total_expenses = total_distribution + total_admin + total_financial + total_other_expenses

    # Calculate profit
    operating_profit = gross_profit + total_other_income - total_expenses
    net_profit_before_tax = operating_profit - total_financial
    net_profit = net_profit_before_tax

    # Calculate ratios
    profit_margin = (net_profit / gross_profit * 100) if gross_profit != 0 else 0
    expense_ratio = (total_expenses / gross_profit * 100) if gross_profit != 0 else 0

    return {
        "profit_loss_account": {
            "gross_profit": gross_profit,
            "add_other_income": {
                "items": other_incomes,
                "total": total_other_income
            },
            "total_income": gross_profit + total_other_income,
            "less_expenses": {
                "distribution_costs": distribution_costs,
                "administrative_expenses": administrative_expenses,
                "financial_expenses": financial_expenses,
                "other_expenses": {
                    "items": other_expenses,
                    "total": total_other_expenses
                }
            },
            "total_expenses": total_expenses
        },
        "net_profit": round(net_profit, 2),
        "profit_margin_percent": round(profit_margin, 2),
        "expense_ratio_percent": round(expense_ratio, 2)
    }


@app.post("/multi-step")
async def multi_step_income_statement(
    sales: float,
    sales_returns: float = 0,
    cost_of_goods_sold: float = 0,
    operating_expenses: List[dict] = [],  # [{"category": "Selling", "amount": x}]
    other_income: float = 0,
    interest_expense: float = 0,
    tax_rate: float = 0
):
    """
    Multi-step Income Statement.
    """
    net_sales = sales - sales_returns
    gross_profit = net_sales - cost_of_goods_sold

    # Operating expenses by category
    selling_expenses = 0
    admin_expenses = 0
    for exp in operating_expenses:
        cat = exp.get("category", "").lower()
        amount = exp.get("amount", 0)
        if "sell" in cat:
            selling_expenses += amount
        else:
            admin_expenses += amount

    total_operating_expenses = selling_expenses + admin_expenses
    operating_income = gross_profit + other_income - total_operating_expenses

    income_before_tax = operating_income - interest_expense
    tax_expense = income_before_tax * (tax_rate / 100) if tax_rate > 0 else 0
    net_income = income_before_tax - tax_expense

    return {
        "revenue": {
            "sales": sales,
            "sales_returns": sales_returns,
            "net_sales": net_sales
        },
        "gross_profit": gross_profit,
        "operating_expenses": {
            "selling_expenses": selling_expenses,
            "administrative_expenses": admin_expenses,
            "total": total_operating_expenses
        },
        "operating_income": operating_income,
        "other_income": other_income,
        "interest_expense": interest_expense,
        "income_before_tax": income_before_tax,
        "tax_expense": tax_expense,
        "net_income": round(net_income, 2)
    }


@app.post("/period-comparison")
async def compare_profit_loss(current_period: dict, previous_period: dict):
    """Compare P&L between two periods."""
    comparison = {}
    for key in ["gross_profit", "net_profit"]:
        if key in current_period and key in previous_period:
            curr = current_period[key]
            prev = previous_period[key]
            change = curr - prev
            pct = (change / prev * 100) if prev != 0 else 0
            comparison[key] = {
                "current": curr,
                "previous": prev,
                "change": round(change, 2),
                "percent_change": round(pct, 2)
            }

    return {"comparison": comparison}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
