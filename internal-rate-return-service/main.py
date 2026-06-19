"""
FinAcc Internal Rate of Return (IRR) Service
Calculates IRR where NPV = 0.
IRR is the discount rate that makes NPV = 0.
"""

import os
import uuid
import math
from datetime import datetime
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "internal-rate-return-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8117"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Internal Rate of Return Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


def calculate_npv_at_rate(initial_investment: float, cash_flows: List[float], rate: float, residual: float = 0) -> float:
    """Helper to calculate NPV at a given rate."""
    pv = sum(cf / math.pow(1 + rate, i + 1) for i, cf in enumerate(cash_flows))
    if residual > 0:
        pv += residual / math.pow(1 + rate, len(cash_flows))
    return pv - initial_investment


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Internal Rate of Return calculation"}


@app.post("/calculate")
async def calculate_irr(
    initial_investment: float,
    cash_flows: List[float],
    residual_value: Optional[float] = 0,
    max_iterations: int = 100,
    tolerance: float = 0.0001
):
    """
    Calculate Internal Rate of Return using Newton-Raphson method.
    IRR is the rate where NPV = 0.
    """
    # Initial guess using average return
    if not cash_flows:
        return {"error": "Cash flows required"}

    avg_cf = sum(cash_flows) / len(cash_flows)
    if initial_investment == 0:
        return {"error": "Initial investment cannot be zero"}

    rate = avg_cf / initial_investment

    # Newton-Raphson iteration
    for _ in range(max_iterations):
        npv = calculate_npv_at_rate(initial_investment, cash_flows, rate, residual_value)

        # Calculate derivative (approximate)
        delta = 0.0001
        npv_plus = calculate_npv_at_rate(initial_investment, cash_flows, rate + delta, residual_value)
        derivative = (npv_plus - npv) / delta

        if abs(derivative) < 1e-10:
            break

        new_rate = rate - npv / derivative

        if abs(new_rate - rate) < tolerance:
            rate = new_rate
            break

        rate = new_rate

        # Safety bounds
        if rate < -0.99:
            rate = -0.5
        elif rate > 10:
            rate = 5

    irr = rate * 100 if rate > 0 else rate

    # Verify with NPV calculation
    final_npv = calculate_npv_at_rate(initial_investment, cash_flows, rate, residual_value)

    return {
        "initial_investment": initial_investment,
        "cash_flows": cash_flows,
        "residual_value": residual_value,
        "irr_percentage": round(irr, 2),
        "irr_decimal": round(rate, 6),
        "npv_at_irr": round(final_npv, 2),
        "formula": "IRR is the rate where NPV = 0"
    }


@app.post("/calculate-bisection")
async def calculate_irr_bisection(
    initial_investment: float,
    cash_flows: List[float],
    residual_value: Optional[float] = 0,
    low_rate: float = 0.0,
    high_rate: float = 1.0,
    iterations: int = 50,
    tolerance: float = 0.0001
):
    """Calculate IRR using bisection method."""
    for _ in range(iterations):
        mid_rate = (low_rate + high_rate) / 2
        npv_mid = calculate_npv_at_rate(initial_investment, cash_flows, mid_rate, residual_value)

        if abs(npv_mid) < tolerance:
            break

        npv_low = calculate_npv_at_rate(initial_investment, cash_flows, low_rate, residual_value)

        if npv_low * npv_mid < 0:
            high_rate = mid_rate
        else:
            low_rate = mid_rate

    irr = mid_rate * 100

    return {
        "initial_investment": initial_investment,
        "irr_percentage": round(irr, 2),
        "npv_at_calculated_irr": round(npv_mid, 2)
    }


@app.post("/compare")
async def compare_irr_projects(
    projects: List[dict]
):
    """Compare IRR of multiple projects."""
    results = []

    for proj in projects:
        name = proj.get("name", "Unknown")
        inv = proj.get("initial_investment", 0)
        cfs = proj.get("cash_flows", [])
        res = proj.get("residual_value", 0)

        if not cfs:
            results.append({"name": name, "error": "No cash flows"})
            continue

        try:
            avg_cf = sum(cfs) / len(cfs)
            if inv == 0:
                results.append({"name": name, "error": "Zero investment"})
                continue

            rate = avg_cf / inv
            for _ in range(50):
                npv = calculate_npv_at_rate(inv, cfs, rate, res)
                delta = 0.0001
                npv_plus = calculate_npv_at_rate(inv, cfs, rate + delta, res)
                deriv = (npv_plus - npv) / delta
                if abs(deriv) < 1e-10:
                    break
                new_rate = rate - npv / deriv
                if abs(new_rate - rate) < 0.0001:
                    break
                rate = max(min(new_rate, 10), -0.5)

            irr = rate * 100
            results.append({"name": name, "irr_percentage": round(irr, 2)})
        except Exception as e:
            results.append({"name": name, "error": str(e)})

    valid = [r for r in results if "error" not in r]
    valid.sort(key=lambda x: x["irr_percentage"], reverse=True)

    return {
        "project_comparison": valid,
        "recommended_project": valid[0]["name"] if valid else None
    }


@app.post("/decision")
async def irr_decision(irr: float, required_rate: float):
    """Make investment decision based on IRR vs required rate."""
    decision = "Accept" if irr > required_rate else "Reject" if irr < required_rate else "Indifferent"
    return {
        "irr": irr,
        "required_rate": required_rate,
        "decision": decision,
        "difference": irr - required_rate
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
