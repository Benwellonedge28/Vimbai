"""
Vimbai Discount Factor Service
Calculates discount factors for present value calculations.
Discount Factor = 1 / (1 + r)^n
where r = rate of interest, n = number of years
"""

import math
import os
import uuid
from datetime import datetime
from typing import Any, Dict, List

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "discount-factor-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8102"))

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

app = FastAPI(title="Vimbai Discount Factor Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class DiscountFactorResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    rate: float  # r - rate of interest
    years: int  # n - number of years
    discount_factor: float
    created_at: datetime = Field(default_factory=datetime.utcnow)


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Discount factor calculations"}


@app.post("/calculate")
async def calculate_discount_factor(rate: float, years: int):
    """
    Calculate discount factor.
    Formula: DF = 1 / (1 + r)^n
    r = rate as decimal (e.g., 10% = 0.10)
    n = number of years
    """
    if years < 0:
        return {"error": "Years cannot be negative"}

    df = 1 / math.pow(1 + rate, years)
    result = DiscountFactorResult(rate=rate, years=years, discount_factor=df)
    return result


@app.post("/calculate-percent")
async def calculate_discount_factor_percent(rate_percent: float, years: int):
    """Calculate discount factor from rate percentage."""
    rate_decimal = rate_percent / 100
    df = 1 / math.pow(1 + rate_decimal, years)
    return {"rate_percent": rate_percent, "rate_decimal": rate_decimal, "years": years, "discount_factor": df}


@app.post("/annuity-factor")
async def calculate_annuity_factor(rate: float, years: int):
    """
    Calculate present value annuity factor.
    PVAF = [1 - 1/(1+r)^n] / r
    """
    if rate == 0:
        pvaf = years
    else:
        pvaf = (1 - (1 / math.pow(1 + rate, years))) / rate

    return {
        "rate": rate,
        "years": years,
        "annuity_factor": pvaf,
        "formula": f"[1 - 1/(1+{rate})^{years}]/{rate} = {pvaf}",
    }


@app.post("/annuity-factor-percent")
async def calculate_annuity_factor_percent(rate_percent: float, years: int):
    """Calculate annuity factor from rate percentage."""
    rate_decimal = rate_percent / 100
    if rate_decimal == 0:
        pvaf = years
    else:
        pvaf = (1 - (1 / math.pow(1 + rate_decimal, years))) / rate_decimal

    return {"rate_percent": rate_percent, "rate_decimal": rate_decimal, "years": years, "annuity_factor": pvaf}


@app.post("/table")
async def generate_discount_factor_table(base_rate: float, years: List[int]):
    """Generate discount factor table for multiple years."""
    factors = []
    for n in years:
        df = 1 / math.pow(1 + base_rate, n)
        factors.append({"year": n, "discount_factor": round(df, 6)})
    return {"rate": base_rate, "factors": factors}


@app.post("/table-range")
async def generate_discount_factor_table_range(rate_start: float, rate_end: float, rate_step: float, years: int):
    """Generate discount factor table for rate range."""
    factors = []
    rate = rate_start
    while rate <= rate_end:
        df = 1 / math.pow(1 + rate, years)
        factors.append({"rate": round(rate, 4), "discount_factor": round(df, 6)})
        rate += rate_step
    return {"years": years, "factors": factors}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
