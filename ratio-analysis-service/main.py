"""
FinAcc Ratio Analysis Service
Comprehensive financial ratio calculations.
Includes liquidity, profitability, leverage, efficiency ratios.
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

SERVICE_NAME = "ratio-analysis-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8130"))
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Ratio Analysis Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Financial ratio analysis"}


@app.post("/liquidity/current-ratio")
async def calculate_current_ratio(current_assets: float, current_liabilities: float):
    """
    Current Ratio = Current Assets / Current Liabilities
    Ideal: > 1.5
    """
    ratio = current_assets / current_liabilities if current_liabilities != 0 else 0
    status = "Good" if ratio >= 1.5 else "Acceptable" if ratio >= 1 else "Poor"

    return {
        "current_assets": current_assets,
        "current_liabilities": current_liabilities,
        "current_ratio": round(ratio, 2),
        "interpretation": status,
        "recommendation": "Maintain adequate working capital" if ratio >= 1.5 else "Improve liquidity position"
    }


@app.post("/liquidity/quick-ratio")
async def calculate_quick_ratio(
    current_assets: float,
    inventory: float,
    current_liabilities: float
):
    """
    Quick Ratio = (Current Assets - Inventory) / Current Liabilities
    Ideal: > 1
    """
    liquid_assets = current_assets - inventory
    ratio = liquid_assets / current_liabilities if current_liabilities != 0 else 0
    status = "Good" if ratio >= 1 else "Poor"

    return {
        "current_assets": current_assets,
        "inventory": inventory,
        "liquid_assets": liquid_assets,
        "current_liabilities": current_liabilities,
        "quick_ratio": round(ratio, 2),
        "interpretation": status
    }


@app.post("/liquidity/cash-ratio")
async def calculate_cash_ratio(cash: float, current_liabilities: float):
    """Cash Ratio = Cash / Current Liabilities"""
    ratio = cash / current_liabilities if current_liabilities != 0 else 0
    return {
        "cash": cash,
        "current_liabilities": current_liabilities,
        "cash_ratio": round(ratio, 2)
    }


@app.post("/liquidity/working-capital")
async def calculate_working_capital(current_assets: float, current_liabilities: float):
    """Working Capital = Current Assets - Current Liabilities"""
    wc = current_assets - current_liabilities
    return {
        "current_assets": current_assets,
        "current_liabilities": current_liabilities,
        "working_capital": wc,
        "interpretation": "Positive" if wc > 0 else "Negative - Liquidity Crisis"
    }


@app.post("/profitability/gross-margin")
async def calculate_gross_margin(revenue: float, cost_of_goods_sold: float):
    """Gross Profit Margin = (Gross Profit / Revenue) × 100"""
    gross_profit = revenue - cost_of_goods_sold
    margin = (gross_profit / revenue * 100) if revenue != 0 else 0
    return {
        "revenue": revenue,
        "cost_of_goods_sold": cost_of_goods_sold,
        "gross_profit": gross_profit,
        "gross_margin_percent": round(margin, 2)
    }


@app.post("/profitability/net-margin")
async def calculate_net_margin(revenue: float, net_profit: float):
    """Net Profit Margin = (Net Profit / Revenue) × 100"""
    margin = (net_profit / revenue * 100) if revenue != 0 else 0
    return {
        "revenue": revenue,
        "net_profit": net_profit,
        "net_margin_percent": round(margin, 2)
    }


@app.post("/profitability/roe")
async def calculate_return_on_equity(net_profit: float, shareholders_equity: float):
    """Return on Equity = (Net Profit / Shareholders' Equity) × 100"""
    roe = (net_profit / shareholders_equity * 100) if shareholders_equity != 0 else 0
    return {
        "net_profit": net_profit,
        "shareholders_equity": shareholders_equity,
        "roe_percent": round(roe, 2)
    }


@app.post("/profitability/roa")
async def calculate_return_on_assets(net_profit: float, total_assets: float):
    """Return on Assets = (Net Profit / Total Assets) × 100"""
    roa = (net_profit / total_assets * 100) if total_assets != 0 else 0
    return {
        "net_profit": net_profit,
        "total_assets": total_assets,
        "roa_percent": round(roa, 2)
    }


@app.post("/leverage/debt-ratio")
async def calculate_debt_ratio(total_debt: float, total_assets: float):
    """Debt Ratio = Total Debt / Total Assets"""
    ratio = (total_debt / total_assets) if total_assets != 0 else 0
    return {
        "total_debt": total_debt,
        "total_assets": total_assets,
        "debt_ratio": round(ratio, 4)
    }


@app.post("/leverage/debt-equity")
async def calculate_debt_equity_ratio(total_debt: float, shareholders_equity: float):
    """Debt to Equity = Total Debt / Shareholders' Equity"""
    ratio = (total_debt / shareholders_equity) if shareholders_equity != 0 else 0
    return {
        "total_debt": total_debt,
        "shareholders_equity": shareholders_equity,
        "debt_equity_ratio": round(ratio, 2)
    }


@app.post("/efficiency/inventory-turnover")
async def calculate_inventory_turnover(cost_of_goods_sold: float, average_inventory: float):
    """Inventory Turnover = COGS / Average Inventory"""
    turnover = cost_of_goods_sold / average_inventory if average_inventory != 0 else 0
    days = 365 / turnover if turnover != 0 else 0
    return {
        "cost_of_goods_sold": cost_of_goods_sold,
        "average_inventory": average_inventory,
        "inventory_turnover": round(turnover, 2),
        "inventory_days": round(days, 1)
    }


@app.post("/efficiency/debtors-turnover")
async def calculate_debtors_turnover(credit_sales: float, average_debtors: float):
    """Debtors Turnover = Credit Sales / Average Debtors"""
    turnover = credit_sales / average_debtors if average_debtors != 0 else 0
    days = 365 / turnover if turnover != 0 else 0
    return {
        "credit_sales": credit_sales,
        "average_debtors": average_debtors,
        "debtors_turnover": round(turnover, 2),
        "debtors_collection_period_days": round(days, 1)
    }


@app.post("/efficiency/creditors-turnover")
async def calculate_creditors_turnover(credit_purchases: float, average_creditors: float):
    """Creditors Turnover = Credit Purchases / Average Creditors"""
    turnover = credit_purchases / average_creditors if average_creditors != 0 else 0
    days = 365 / turnover if turnover != 0 else 0
    return {
        "credit_purchases": credit_purchases,
        "average_creditors": average_creditors,
        "creditors_turnover": round(turnover, 2),
        "creditors_payment_period_days": round(days, 1)
    }


@app.post("/comprehensive-analysis")
async def comprehensive_ratio_analysis(
    current_assets: float,
    current_liabilities: float,
    inventory: float,
    cash: float,
    total_assets: float,
    total_debt: float,
    shareholders_equity: float,
    revenue: float,
    cost_of_goods_sold: float,
    net_profit: float,
    credit_sales: float,
    average_debtors: float
):
    """Calculate all key ratios in one call."""
    # Liquidity
    current_ratio = current_assets / current_liabilities if current_liabilities != 0 else 0
    quick_ratio = (current_assets - inventory) / current_liabilities if current_liabilities != 0 else 0
    working_capital = current_assets - current_liabilities

    # Profitability
    gross_profit = revenue - cost_of_goods_sold
    gross_margin = (gross_profit / revenue * 100) if revenue != 0 else 0
    net_margin = (net_profit / revenue * 100) if revenue != 0 else 0
    roe = (net_profit / shareholders_equity * 100) if shareholders_equity != 0 else 0
    roa = (net_profit / total_assets * 100) if total_assets != 0 else 0

    # Leverage
    debt_ratio = (total_debt / total_assets) if total_assets != 0 else 0
    debt_equity = (total_debt / shareholders_equity) if shareholders_equity != 0 else 0

    # Efficiency
    debtors_turnover = credit_sales / average_debtors if average_debtors != 0 else 0
    collection_period = 365 / debtors_turnover if debtors_turnover != 0 else 0

    return {
        "liquidity_ratios": {
            "current_ratio": round(current_ratio, 2),
            "quick_ratio": round(quick_ratio, 2),
            "cash_ratio": round(cash / current_liabilities, 2) if current_liabilities != 0 else 0,
            "working_capital": working_capital
        },
        "profitability_ratios": {
            "gross_margin_percent": round(gross_margin, 2),
            "net_margin_percent": round(net_margin, 2),
            "roe_percent": round(roe, 2),
            "roa_percent": round(roa, 2)
        },
        "leverage_ratios": {
            "debt_ratio": round(debt_ratio, 4),
            "debt_equity_ratio": round(debt_equity, 2)
        },
        "efficiency_ratios": {
            "debtors_turnover": round(debtors_turnover, 2),
            "collection_period_days": round(collection_period, 1)
        }
    }


@app.post("/compare-periods")
async def compare_ratios(period_a: dict, period_b: dict):
    """Compare ratios between two periods."""
    comparison = {}
    for key in period_a:
        if isinstance(period_a[key], (int, float)) and isinstance(period_b.get(key), (int, float)):
            diff = period_b[key] - period_a[key]
            pct_change = (diff / period_a[key] * 100) if period_a[key] != 0 else 0
            comparison[key] = {
                "period_a": period_a[key],
                "period_b": period_b[key],
                "change": round(diff, 2),
                "percent_change": round(pct_change, 2)
            }
    return {"comparison": comparison}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
