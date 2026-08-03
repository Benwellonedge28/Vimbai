"""
Vimbai Payback Period Service
Calculates payback period using non-discounted cash flows.
Payback Period = Time taken to recover initial investment
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "payback-period-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8114"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Payback Period Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Payback period calculation"}


@app.post("/calculate")
async def calculate_payback_period(initial_investment: float, annual_cash_flows: List[float]):
    """
    Calculate payback period.
    Payback Period = Initial Investment / Annual Cash Flow (equal flows)
    Or calculated by cumulative cash flows (unequal flows)
    """
    # Check if all cash flows are equal
    if len(set(annual_cash_flows)) == 1:
        annual_cf = annual_cash_flows[0]
        if annual_cf <= 0:
            return {"error": "Annual cash flow must be positive for payback calculation"}
        payback = initial_investment / annual_cf
        return {
            "initial_investment": initial_investment,
            "annual_cash_flows": annual_cash_flows,
            "equal_cash_flows": True,
            "payback_period": round(payback, 2),
            "payback_years": f"{int(payback)} years and {(payback % 1) * 12:.1f} months"
        }

    # Unequal cash flows - calculate cumulative
    cumulative = 0
    years = []
    for i, cf in enumerate(annual_cash_flows, 1):
        cumulative += cf
        years.append({"year": i, "cash_flow": cf, "cumulative": round(cumulative, 2)})

        if cumulative >= initial_investment:
            # Interpolate for partial year
            prev_cumulative = cumulative - cf
            remaining = initial_investment - prev_cumulative
            partial_year = remaining / cf if cf != 0 else 0
            payback = (i - 1) + partial_year

            return {
                "initial_investment": initial_investment,
                "cash_flow_schedule": years,
                "equal_cash_flows": False,
                "payback_period": round(payback, 2),
                "payback_years": f"{int(payback)} years and {(payback % 1) * 12:.1f} months",
                "payback_achieved_in_year": i
            }

    # Payback never achieved
    return {
        "initial_investment": initial_investment,
        "cash_flow_schedule": years,
        "equal_cash_flows": False,
        "payback_period": None,
        "message": "Payback not achieved within given cash flow period"
    }


@app.post("/compare")
async def compare_payback_periods(
    projects: List[dict]  # [{"name": "A", "initial_investment": x, "cash_flows": [y1, y2, ...]}]
):
    """Compare payback periods of multiple projects."""
    results = []
    for proj in projects:
        name = proj.get("name", "Unknown")
        inv = proj.get("initial_investment", 0)
        cfs = proj.get("cash_flows", [])

        if not cfs:
            results.append({"name": name, "error": "No cash flows provided"})
            continue

        if len(set(cfs)) == 1:
            payback = inv / cfs[0] if cfs[0] > 0 else None
        else:
            cumulative = 0
            payback = None
            for i, cf in enumerate(cfs, 1):
                cumulative += cf
                if cumulative >= inv:
                    prev = cumulative - cf
                    remaining = inv - prev
                    payback = (i - 1) + (remaining / cf if cf != 0 else 0)
                    break

        results.append({"name": name, "payback_period": round(payback, 2) if payback else None})

    # Sort by payback period
    valid_results = [r for r in results if r.get("payback_period") is not None]
    valid_results.sort(key=lambda x: x["payback_period"])

    return {
        "project_comparison": valid_results,
        "recommended_project": valid_results[0]["name"] if valid_results else None
    }


@app.post("/average-cash-flow")
async def average_cash_flow_payback(initial_investment: float, total_cash_flows: float, years: int):
    """Calculate payback using average annual cash flow."""
    if years == 0:
        return {"error": "Years must be positive"}

    avg_cf = total_cash_flows / years
    payback = initial_investment / avg_cf if avg_cf > 0 else None

    return {
        "initial_investment": initial_investment,
        "total_cash_flows": total_cash_flows,
        "years": years,
        "average_annual_cash_flow": round(avg_cf, 2),
        "payback_period": round(payback, 2) if payback else None
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
