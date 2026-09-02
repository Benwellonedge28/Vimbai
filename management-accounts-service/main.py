"""
Management Accounts Service
Port: 8281
Management accounts preparation
"""

from datetime import datetime
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Management Accounts Service", version="1.0.0")


class ManagementAccountsRequest(BaseModel):
    company_id: str
    period: str
    revenue_items: Dict[str, float]
    cost_items: Dict[str, float]
    balance_sheet_items: Dict[str, float]


class ManagementAccountsResponse(BaseModel):
    company_id: str
    period: str
    pnl_statement: Dict[str, Any]
    balance_sheet: Dict[str, Any]
    working_capital: Dict[str, Any]
    key_ratios: Dict[str, float]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "management-accounts", "version": "1.0.0"}


@app.post("/prepare", response_model=ManagementAccountsResponse)
async def prepare_management_accounts(request: ManagementAccountsRequest):
    logger.info("Preparing management accounts", company=request.company_id)

    total_revenue = sum(request.revenue_items.values())
    total_costs = sum(request.cost_items.values())
    ebitda = total_revenue - total_costs

    pnl_statement = {
        "revenue": round(total_revenue, 2),
        "cogs": round(request.cost_items.get("cogs", 0), 2),
        "gross_profit": round(total_revenue - request.cost_items.get("cogs", 0), 2),
        "opex": round(request.cost_items.get("opex", 0), 2),
        "ebitda": round(ebitda, 2),
        "gross_margin": (
            round((total_revenue - request.cost_items.get("cogs", 0)) / total_revenue * 100, 2) if total_revenue else 0
        ),
    }

    total_assets = sum(request.balance_sheet_items.get(k, 0) for k in ["current_assets", "non_current_assets"])
    total_liabilities = sum(
        request.balance_sheet_items.get(k, 0) for k in ["current_liabilities", "non_current_liabilities"]
    )

    balance_sheet = {
        "total_assets": round(total_assets, 2),
        "total_liabilities": round(total_liabilities, 2),
        "net_assets": round(total_assets - total_liabilities, 2),
    }

    current_assets = request.balance_sheet_items.get("current_assets", 0)
    current_liabilities = request.balance_sheet_items.get("current_liabilities", 1)

    working_capital = {
        "working_capital": round(current_assets - current_liabilities, 2),
        "current_ratio": round(current_assets / current_liabilities, 2) if current_liabilities else 0,
    }

    key_ratios = {
        "gross_margin_pct": pnl_statement["gross_margin"],
        "ebitda_margin_pct": round(ebitda / total_revenue * 100, 2) if total_revenue else 0,
        "current_ratio": working_capital["current_ratio"],
    }

    return ManagementAccountsResponse(
        company_id=request.company_id,
        period=request.period,
        pnl_statement=pnl_statement,
        balance_sheet=balance_sheet,
        working_capital=working_capital,
        key_ratios=key_ratios,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8281)
