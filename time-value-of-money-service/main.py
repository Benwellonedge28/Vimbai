"""
Vimbai Time Value of Money Service
Handles all time value of money calculations.
"""

import os
import uuid
import math
from datetime import datetime
from typing import Any, Dict, Optional

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "time-value-of-money-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8105"))

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Time Value of Money Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Time value of money calculations"}


@app.post("/future-value")
async def calculate_future_value(principal: float, rate: float, years: int, compounding: str = "annual"):
    """
    Calculate future value.
    FV = PV × (1 + r)^n
    """
    if compounding == "annual":
        factor = math.pow(1 + rate, years)
    elif compounding == "quarterly":
        factor = math.pow(1 + rate / 4, years * 4)
    elif compounding == "monthly":
        factor = math.pow(1 + rate / 12, years * 12)
    else:
        factor = math.pow(1 + rate, years)

    fv = principal * factor
    return {
        "principal": principal,
        "rate": rate,
        "years": years,
        "compounding": compounding,
        "future_value": round(fv, 2),
        "interest_earned": round(fv - principal, 2)
    }


@app.post("/present-value")
async def calculate_pv_time_value(future_value: float, rate: float, years: int):
    """Calculate present value."""
    pv = future_value / math.pow(1 + rate, years)
    return {
        "future_value": future_value,
        "rate": rate,
        "years": years,
        "present_value": round(pv, 2)
    }


@app.post("/compound-interest")
async def calculate_compound_interest(principal: float, rate: float, years: int):
    """Calculate compound interest amount."""
    amount = principal * math.pow(1 + rate, years)
    interest = amount - principal
    return {
        "principal": principal,
        "rate": rate,
        "years": years,
        "total_amount": round(amount, 2),
        "compound_interest": round(interest, 2)
    }


@app.post("/simple-interest")
async def calculate_simple_interest(principal: float, rate: float, years: int):
    """Calculate simple interest."""
    interest = principal * rate * years
    amount = principal + interest
    return {
        "principal": principal,
        "rate": rate,
        "years": years,
        "simple_interest": round(interest, 2),
        "total_amount": round(amount, 2)
    }


@app.post("/effective-annual-rate")
async def calculate_effective_annual_rate(nominal_rate: float, compounding_per_year: int):
    """Calculate effective annual rate from nominal rate."""
    ear = math.pow(1 + nominal_rate / compounding_per_year, compounding_per_year) - 1
    return {
        "nominal_rate": nominal_rate,
        "compounding_per_year": compounding_per_year,
        "effective_annual_rate": round(ear, 6),
        "effective_rate_percent": f"{ear * 100:.2f}%"
    }


@app.post("/pv-annuity")
async def calculate_pv_annuity(payment: float, rate: float, years: int):
    """Calculate present value of annuity."""
    if rate == 0:
        pv = payment * years
    else:
        pv = payment * (1 - 1 / math.pow(1 + rate, years)) / rate
    return {
        "payment": payment,
        "rate": rate,
        "years": years,
        "present_value_annuity": round(pv, 2)
    }


@app.post("/fv-annuity")
async def calculate_fv_annuity(payment: float, rate: float, years: int):
    """Calculate future value of annuity."""
    if rate == 0:
        fv = payment * years
    else:
        fv = payment * (math.pow(1 + rate, years) - 1) / rate
    return {
        "payment": payment,
        "rate": rate,
        "years": years,
        "future_value_annuity": round(fv, 2)
    }


@app.post("/loan-repayment")
async def calculate_loan_repayment(principal: float, rate: float, years: int):
    """Calculate annual loan repayment."""
    if rate == 0:
        repayment = principal / years
    else:
        repayment = principal * rate * math.pow(1 + rate, years) / (math.pow(1 + rate, years) - 1)
    return {
        "principal": principal,
        "rate": rate,
        "years": years,
        "annual_repayment": round(repayment, 2),
        "total_paid": round(repayment * years, 2),
        "total_interest": round(repayment * years - principal, 2)
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
