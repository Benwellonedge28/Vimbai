"""
Vimbai Accounting Rate of Return (ARR) Service
Calculates ARR using average profit and average capital.
ARR = (Average Annual Profit / Average Investment) × 100
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "accounting-rate-return-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8115"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Accounting Rate of Return Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Accounting Rate of Return (ARR) calculation"}


@app.post("/calculate")
async def calculate_arr(
    initial_investment: float,
    residual_value: float,
    total_profits: float,
    years: int
):
    """
    Calculate Accounting Rate of Return.
    ARR = (Average Annual Profit / Average Investment) × 100
    Average Investment = (Initial Cost + Residual Value) / 2
    """
    if years == 0:
        return {"error": "Years must be positive"}

    avg_annual_profit = total_profits / years
    avg_investment = (initial_investment + residual_value) / 2
    arr = (avg_annual_profit / avg_investment * 100) if avg_investment != 0 else 0

    return {
        "initial_investment": initial_investment,
        "residual_value": residual_value,
        "total_profits": total_profits,
        "years": years,
        "average_annual_profit": round(avg_annual_profit, 2),
        "average_investment": round(avg_investment, 2),
        "arr_percentage": round(arr, 2),
        "arr_decimal": round(arr / 100, 4)
    }


@app.post("/from-annual-profits")
async def calculate_arr_from_annual_profits(
    annual_profits: List[float],
    initial_investment: float,
    residual_value: float = 0
):
    """Calculate ARR from list of annual profits."""
    if not annual_profits:
        return {"error": "Annual profits required"}

    avg_profit = sum(annual_profits) / len(annual_profits)
    avg_investment = (initial_investment + residual_value) / 2
    arr = (avg_profit / avg_investment * 100) if avg_investment != 0 else 0

    return {
        "annual_profits": annual_profits,
        "number_of_years": len(annual_profits),
        "total_profits": sum(annual_profits),
        "average_annual_profit": round(avg_profit, 2),
        "initial_investment": initial_investment,
        "residual_value": residual_value,
        "average_investment": round(avg_investment, 2),
        "arr_percentage": round(arr, 2)
    }


@app.post("/average-profit")
async def calculate_average_profit(total_profits: float, years: int):
    """Calculate average annual profit."""
    if years == 0:
        return {"error": "Years must be positive"}
    avg = total_profits / years
    return {
        "total_profits": total_profits,
        "years": years,
        "average_annual_profit": round(avg, 2)
    }


@app.post("/average-capital")
async def calculate_average_capital(initial_investment: float, residual_value: float):
    """Calculate average capital employed."""
    avg = (initial_investment + residual_value) / 2
    return {
        "initial_investment": initial_investment,
        "residual_value": residual_value,
        "average_capital": round(avg, 2)
    }


@app.post("/compare")
async def compare_arr_projects(
    projects: List[dict]  # [{"name": "A", "initial_investment": x, "residual_value": y, "annual_profits": [p1, p2, ...]}]
):
    """Compare ARR of multiple projects."""
    results = []
    for proj in projects:
        name = proj.get("name", "Unknown")
        inv = proj.get("initial_investment", 0)
        res = proj.get("residual_value", 0)
        profits = proj.get("annual_profits", [])

        if not profits:
            results.append({"name": name, "error": "No profits provided"})
            continue

        avg_profit = sum(profits) / len(profits)
        avg_capital = (inv + res) / 2
        arr = (avg_profit / avg_capital * 100) if avg_capital != 0 else 0

        results.append({
            "name": name,
            "average_annual_profit": round(avg_profit, 2),
            "average_capital": round(avg_capital, 2),
            "arr_percentage": round(arr, 2)
        })

    # Sort by ARR descending
    valid_results = [r for r in results if "error" not in r]
    valid_results.sort(key=lambda x: x["arr_percentage"], reverse=True)

    return {
        "project_comparison": valid_results,
        "recommended_project": valid_results[0]["name"] if valid_results else None
    }


@app.post("/decision")
async def arr_decision(
    arr: float,
    target_return: float
):
    """Make decision based on ARR vs target return."""
    decision = "Accept" if arr > target_return else "Reject" if arr < target_return else "Indifferent"
    return {
        "calculated_arr": arr,
        "target_return": target_return,
        "decision": decision,
        "difference": arr - target_return
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
