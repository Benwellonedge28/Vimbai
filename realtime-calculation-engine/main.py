"""Vimbai Realtime Calculation Engine - High-speed financial calculations. Port: 8371"""

import math
import os
import time
from datetime import datetime, timezone
from typing import Any, Dict, List

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel

SERVICE_NAME = "realtime-calculation-engine"
PORT = int(os.getenv("PORT", "8371"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Realtime Calculation Engine", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="realtime-calculation-engine", instrument_app=app)
except ImportError:
    TRACER = None


class NPVRequest(BaseModel):
    initial_investment: float
    cash_flows: List[float]
    discount_rate: float


class IRRRequest(BaseModel):
    initial_investment: float
    cash_flows: List[float]


class DepreciationRequest(BaseModel):
    cost: float
    salvage_value: float
    useful_life: int
    method: str = "straight_line"  # straight_line, declining_balance, sum_of_years


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/npv")
async def calculate_npv(req: NPVRequest):
    npv = -req.initial_investment
    for i, cf in enumerate(req.cash_flows, 1):
        npv += cf / ((1 + req.discount_rate / 100) ** i)
    return {"npv": round(npv, 2), "profitable": npv > 0, "rate": req.discount_rate}


@app.post("/irr")
async def calculate_irr(req: IRRRequest):
    cash_flows = [-req.initial_investment] + req.cash_flows
    # Newton-Raphson method
    rate = 0.1
    for _ in range(100):
        npv = sum(cf / ((1 + rate) ** i) for i, cf in enumerate(cash_flows))
        dnpv = sum(-i * cf / ((1 + rate) ** (i + 1)) for i, cf in enumerate(cash_flows))
        if abs(dnpv) < 1e-10:
            break
        rate -= npv / dnpv
        if abs(npv) < 1e-6:
            break
    return {"irr": round(rate * 100, 4), "npv_at_irr": round(npv, 6)}


@app.post("/depreciation")
async def calculate_depreciation(req: DepreciationRequest):
    depreciable = req.cost - req.salvage_value
    if req.method == "straight_line":
        annual = depreciable / req.useful_life
        schedule = [
            {"year": y, "depreciation": annual, "accumulated": annual * y, "book_value": req.cost - annual * y}
            for y in range(1, req.useful_life + 1)
        ]
    elif req.method == "declining_balance":
        rate = 2 / req.useful_life  # double declining
        schedule = []
        book_value = req.cost
        for y in range(1, req.useful_life + 1):
            dep = book_value * rate
            if book_value - dep < req.salvage_value:
                dep = book_value - req.salvage_value
            book_value -= dep
            schedule.append(
                {"year": y, "depreciation": dep, "accumulated": req.cost - book_value, "book_value": book_value}
            )
    else:  # sum_of_years
        sod = req.useful_life * (req.useful_life + 1) / 2
        schedule = []
        acc = 0
        for y in range(1, req.useful_life + 1):
            dep = depreciable * (req.useful_life - y + 1) / sod
            acc += dep
            schedule.append({"year": y, "depreciation": dep, "accumulated": acc, "book_value": req.cost - acc})
    return {"method": req.method, "total_depreciable": depreciable, "schedule": schedule}


@app.post("/amortize")
async def calculate_amortization(principal: float, annual_rate: float, years: int):
    monthly_rate = annual_rate / 100 / 12
    n = years * 12
    if monthly_rate == 0:
        payment = principal / n
    else:
        payment = principal * (monthly_rate * (1 + monthly_rate) ** n) / ((1 + monthly_rate) ** n - 1)
    schedule = []
    balance = principal
    for m in range(1, n + 1):
        interest = balance * monthly_rate
        principal_paid = payment - interest
        balance -= principal_paid
        schedule.append(
            {
                "month": m,
                "payment": round(payment, 2),
                "interest": round(interest, 2),
                "principal": round(principal_paid, 2),
                "balance": round(max(0, balance), 2),
            }
        )
    return {
        "monthly_payment": round(payment, 2),
        "total_paid": round(payment * n, 2),
        "total_interest": round(payment * n - principal, 2),
        "schedule": schedule,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
