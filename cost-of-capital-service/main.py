"""
FinAcc Cost of Capital Service
Handles rate of interest/cost of capital calculations.
"""

import os
import uuid
import math
from datetime import datetime
from typing import Any, Dict, List

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "cost-of-capital-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8104"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Cost of Capital Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Cost of capital/rate of interest calculations"}


@app.post("/calculate")
async def calculate_cost_of_capital(
    interest_rate: float,
    risk_premium: float = 0
):
    """Calculate cost of capital including risk premium."""
    total_rate = interest_rate + risk_premium
    return {
        "base_rate": interest_rate,
        "risk_premium": risk_premium,
        "total_cost_of_capital": total_rate,
        "as_percentage": f"{total_rate * 100}%"
    }


@app.post("/from-percent")
async def cost_of_capital_percent(
    interest_rate_percent: float,
    risk_premium_percent: float = 0
):
    """Calculate cost of capital from percentages."""
    rate_decimal = (interest_rate_percent + risk_premium_percent) / 100
    return {
        "interest_rate_percent": interest_rate_percent,
        "risk_premium_percent": risk_premium_percent,
        "total_rate_decimal": rate_decimal,
        "total_rate_percent": interest_rate_percent + risk_premium_percent
    }


@app.post("/wacc")
async def calculate_wacc(
    equity_value: float,
    cost_of_equity: float,
    debt_value: float,
    cost_of_debt: float,
    tax_rate: float = 0
):
    """Calculate Weighted Average Cost of Capital (WACC)."""
    total_capital = equity_value + debt_value
    if total_capital == 0:
        return {"error": "Total capital cannot be zero"}

    # After-tax cost of debt
    after_tax_cost_debt = cost_of_debt * (1 - tax_rate / 100) if tax_rate > 0 else cost_of_debt

    # Weights
    equity_weight = equity_value / total_capital
    debt_weight = debt_value / total_capital

    # WACC
    wacc = (equity_value * cost_of_equity + debt_value * after_tax_cost_debt) / total_capital

    return {
        "equity_value": equity_value,
        "debt_value": debt_value,
        "total_capital": total_capital,
        "cost_of_equity": cost_of_equity,
        "cost_of_debt": cost_of_debt,
        "after_tax_cost_debt": after_tax_cost_debt,
        "equity_weight": equity_weight,
        "debt_weight": debt_weight,
        "wacc": round(wacc, 6),
        "wacc_percent": f"{wacc * 100:.2f}%"
    }


@app.post("/simple-rate")
async def simple_rate(
    principal: float,
    amount_after: float,
    years: int
):
    """Calculate simple rate from principal and final amount."""
    if years == 0 or principal == 0:
        return {"error": "Years and principal must be positive"}

    rate = ((amount_after / principal) - 1) / years
    return {
        "principal": principal,
        "amount_after": amount_after,
        "years": years,
        "simple_rate": round(rate, 6),
        "rate_percent": f"{rate * 100:.2f}%"
    }


@app.post("/compound-rate")
async def compound_rate(
    present_value: float,
    future_value: float,
    years: int
):
    """Calculate compound annual growth rate (CAGR)."""
    if years == 0 or present_value == 0:
        return {"error": "Years and present value must be positive"}

    rate = math.pow(future_value / present_value, 1 / years) - 1
    return {
        "present_value": present_value,
        "future_value": future_value,
        "years": years,
        "compound_rate": round(rate, 6),
        "rate_percent": f"{rate * 100:.2f}%"
    }


@app.get("/list")
async def list_standard_rates():
    """List common standard rates used in appraisal."""
    return {
        "standard_rates": [
            {"name": "Treasury Rate", "description": "Risk-free rate"},
            {"name": "Cost of Debt", "description": "Interest rate on borrowed capital"},
            {"name": "Cost of Equity", "description": "Required return by shareholders"},
            {"name": "WACC", "description": "Weighted Average Cost of Capital"}
        ]
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
