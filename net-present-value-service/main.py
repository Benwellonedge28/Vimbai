"""
Vimbai Net Present Value (NPV) Service
Calculates NPV of investment projects.
NPV = Sum of PV of inflows - Initial Investment
"""

import math
import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "net-present-value-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8116"))

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

app = FastAPI(title="Vimbai Net Present Value Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Net Present Value calculation"}


@app.post("/calculate")
async def calculate_npv(
    initial_investment: float, cash_flows: List[float], discount_rate: float, residual_value: Optional[float] = 0
):
    """
    Calculate Net Present Value.
    NPV = PV of inflows - Initial Investment
    """
    pv_inflows = 0
    details = []

    # PV of each year's cash flow
    for i, cf in enumerate(cash_flows, 1):
        df = 1 / math.pow(1 + discount_rate, i)
        pv = cf * df
        pv_inflows += pv
        details.append({"year": i, "cash_flow": cf, "discount_factor": round(df, 4), "present_value": round(pv, 2)})

    # PV of residual value
    if residual_value and residual_value > 0:
        years = len(cash_flows)
        df = 1 / math.pow(1 + discount_rate, years)
        pv_residual = residual_value * df
        pv_inflows += pv_residual
        details.append(
            {
                "year": years,
                "description": "Residual Value",
                "cash_flow": residual_value,
                "discount_factor": round(df, 4),
                "present_value": round(pv_residual, 2),
            }
        )

    npv = pv_inflows - initial_investment

    return {
        "initial_investment": initial_investment,
        "discount_rate": discount_rate,
        "residual_value": residual_value,
        "cash_flow_details": details,
        "total_pv_inflows": round(pv_inflows, 2),
        "net_present_value": round(npv, 2),
        "decision": (
            "Accept - Positive NPV" if npv > 0 else "Reject - Negative NPV" if npv < 0 else "Indifferent - Zero NPV"
        ),
    }


@app.post("/calculate-percent")
async def calculate_npv_percent(
    initial_investment: float,
    cash_flows: List[float],
    discount_rate_percent: float,
    residual_value: Optional[float] = 0,
):
    """Calculate NPV with discount rate as percentage."""
    rate = discount_rate_percent / 100
    pv_inflows = 0

    for i, cf in enumerate(cash_flows, 1):
        df = 1 / math.pow(1 + rate, i)
        pv_inflows += cf * df

    if residual_value and residual_value > 0:
        df = 1 / math.pow(1 + rate, len(cash_flows))
        pv_inflows += residual_value * df

    npv = pv_inflows - initial_investment

    return {
        "initial_investment": initial_investment,
        "discount_rate_percent": discount_rate_percent,
        "discount_rate_decimal": rate,
        "residual_value": residual_value,
        "net_present_value": round(npv, 2),
        "decision": "Accept" if npv > 0 else "Reject" if npv < 0 else "Indifferent",
    }


@app.post("/compare")
async def compare_npv_projects(
    projects: List[
        dict
    ],  # [{"name": "A", "initial_investment": x, "cash_flows": [y1, y2, ...], "discount_rate": r, "residual_value": v}]
):
    """Compare NPV of multiple projects."""
    results = []

    for proj in projects:
        name = proj.get("name", "Unknown")
        inv = proj.get("initial_investment", 0)
        cfs = proj.get("cash_flows", [])
        rate = proj.get("discount_rate", 0)
        res = proj.get("residual_value", 0)

        if not cfs or rate == 0:
            results.append({"name": name, "error": "Invalid data"})
            continue

        pv = sum(cf / math.pow(1 + rate, i + 1) for i, cf in enumerate(cfs))
        if res > 0:
            pv += res / math.pow(1 + rate, len(cfs))
        npv = pv - inv

        results.append({"name": name, "initial_investment": inv, "net_present_value": round(npv, 2)})

    # Sort by NPV descending
    valid = [r for r in results if "error" not in r]
    valid.sort(key=lambda x: x["net_present_value"], reverse=True)

    return {"project_comparison": valid, "recommended_project": valid[0]["name"] if valid else None}


@app.post("/profitability-index")
async def calculate_profitability_index(
    initial_investment: float, cash_flows: List[float], discount_rate: float, residual_value: Optional[float] = 0
):
    """Calculate Profitability Index (PI)."""
    pv_inflows = sum(cf / math.pow(1 + discount_rate, i + 1) for i, cf in enumerate(cash_flows))
    if residual_value and residual_value > 0:
        pv_inflows += residual_value / math.pow(1 + discount_rate, len(cash_flows))

    pi = pv_inflows / initial_investment if initial_investment != 0 else 0

    return {
        "initial_investment": initial_investment,
        "pv_of_inflows": round(pv_inflows, 2),
        "profitability_index": round(pi, 4),
        "interpretation": "Accept" if pi > 1 else "Reject" if pi < 1 else "Indifferent",
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
