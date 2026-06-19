"""
FinAcc Discounted Payback Period Service
Calculates payback period using discounted cash flows.
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

SERVICE_NAME = "discounted-payback-period-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8118"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Discounted Payback Period Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Discounted payback period calculation"}


@app.post("/calculate")
async def calculate_discounted_payback(
    initial_investment: float,
    cash_flows: List[float],
    discount_rate: float
):
    """
    Calculate Discounted Payback Period.
    Uses discounted cash flows instead of nominal cash flows.
    """
    discounted_cumulative = 0
    schedule = []

    for i, cf in enumerate(cash_flows, 1):
        df = 1 / math.pow(1 + discount_rate, i)
        pv = cf * df
        discounted_cumulative += pv

        schedule.append({
            "year": i,
            "cash_flow": cf,
            "discount_factor": round(df, 4),
            "present_value": round(pv, 2),
            "cumulative_pv": round(discounted_cumulative, 2)
        })

        if discounted_cumulative >= initial_investment:
            # Interpolate for partial year
            prev_cumulative = discounted_cumulative - pv
            remaining = initial_investment - prev_cumulative
            fraction = remaining / pv if pv != 0 else 0
            payback = (i - 1) + fraction

            return {
                "initial_investment": initial_investment,
                "discount_rate": discount_rate,
                "discounted_cash_flow_schedule": schedule,
                "discounted_payback_period": round(payback, 2),
                "payback_achieved_in_year": i,
                "note": "More conservative than simple payback as it considers time value of money"
            }

    return {
        "initial_investment": initial_investment,
        "discount_rate": discount_rate,
        "discounted_cash_flow_schedule": schedule,
        "discounted_payback_period": None,
        "total_pv_of_cash_flows": round(discounted_cumulative, 2),
        "message": "Discounted payback not achieved within given period"
    }


@app.post("/calculate-percent")
async def calculate_discounted_payback_percent(
    initial_investment: float,
    cash_flows: List[float],
    discount_rate_percent: float
):
    """Calculate discounted payback with rate as percentage."""
    rate = discount_rate_percent / 100
    discounted_cumulative = 0
    schedule = []

    for i, cf in enumerate(cash_flows, 1):
        df = 1 / math.pow(1 + rate, i)
        pv = cf * df
        discounted_cumulative += pv

        schedule.append({
            "year": i,
            "cash_flow": cf,
            "discount_factor": round(df, 4),
            "present_value": round(pv, 2)
        })

        if discounted_cumulative >= initial_investment:
            prev = discounted_cumulative - pv
            remaining = initial_investment - prev
            payback = (i - 1) + (remaining / pv if pv != 0 else 0)

            return {
                "initial_investment": initial_investment,
                "discount_rate_percent": discount_rate_percent,
                "discounted_payback_period": round(payback, 2),
                "schedule": schedule
            }

    return {
        "initial_investment": initial_investment,
        "discount_rate_percent": discount_rate_percent,
        "discounted_payback_period": None,
        "message": "Discounted payback not achieved"
    }


@app.post("/compare")
async def compare_discounted_payback(
    projects: List[dict]
):
    """Compare discounted payback periods of multiple projects."""
    results = []

    for proj in projects:
        name = proj.get("name", "Unknown")
        inv = proj.get("initial_investment", 0)
        cfs = proj.get("cash_flows", [])
        rate = proj.get("discount_rate", 0)

        if not cfs or rate == 0:
            results.append({"name": name, "error": "Invalid data"})
            continue

        cum = 0
        payback = None
        for i, cf in enumerate(cfs, 1):
            pv = cf / math.pow(1 + rate, i)
            cum += pv
            if cum >= inv:
                prev = cum - pv
                remaining = inv - prev
                payback = (i - 1) + (remaining / pv if pv != 0 else 0)
                break

        results.append({
            "name": name,
            "discounted_payback_period": round(payback, 2) if payback else None
        })

    valid = [r for r in results if r.get("discounted_payback_period") is not None]
    valid.sort(key=lambda x: x["discounted_payback_period"])

    return {
        "project_comparison": valid,
        "recommended_project": valid[0]["name"] if valid else None
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
