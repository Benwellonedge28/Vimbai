"""
Vimbai Present Value Service
Calculates present value of future cash flows.
PV = FV / (1 + r)^n
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

SERVICE_NAME = "present-value-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8103"))

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

app = FastAPI(title="Vimbai Present Value Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class PresentValueResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    future_value: float
    rate: float
    years: int
    present_value: float
    discount_factor: float
    created_at: datetime = Field(default_factory=datetime.utcnow)


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Present value calculations"}


@app.post("/calculate")
async def calculate_present_value(future_value: float, rate: float, years: int):
    """
    Calculate present value.
    Formula: PV = FV / (1 + r)^n
    """
    df = 1 / math.pow(1 + rate, years)
    pv = future_value * df

    result = PresentValueResult(future_value=future_value, rate=rate, years=years, present_value=pv, discount_factor=df)
    return result


@app.post("/calculate-percent")
async def calculate_present_value_percent(future_value: float, rate_percent: float, years: int):
    """Calculate present value from rate percentage."""
    rate_decimal = rate_percent / 100
    df = 1 / math.pow(1 + rate_decimal, years)
    pv = future_value * df

    return {
        "future_value": future_value,
        "rate_percent": rate_percent,
        "rate_decimal": rate_decimal,
        "years": years,
        "discount_factor": round(df, 6),
        "present_value": round(pv, 2),
    }


@app.post("/cash-flows")
async def calculate_pv_of_cash_flows(cash_flows: List[float], rate: float):
    """
    Calculate present value of a series of cash flows.
    PV = CF1/(1+r)^1 + CF2/(1+r)^2 + ... + CFn/(1+r)^n
    """
    pv_list = []
    total_pv = 0

    for i, cf in enumerate(cash_flows, 1):
        df = 1 / math.pow(1 + rate, i)
        pv_cf = cf * df
        pv_list.append({"year": i, "cash_flow": cf, "discount_factor": round(df, 6), "present_value": round(pv_cf, 2)})
        total_pv += pv_cf

    return {
        "cash_flows": cash_flows,
        "rate": rate,
        "present_values": pv_list,
        "total_present_value": round(total_pv, 2),
    }


@app.post("/cash-flows-with-initial")
async def calculate_pv_with_initial_investment(
    initial_investment: float, cash_flows: List[float], rate: float, residual_value: Optional[float] = 0
):
    """Calculate NPV (PV of inflows - initial investment)."""
    pv_inflows = 0
    details = []

    # PV of cash flows
    for i, cf in enumerate(cash_flows, 1):
        df = 1 / math.pow(1 + rate, i)
        pv_cf = cf * df
        pv_inflows += pv_cf
        details.append({"year": i, "cash_flow": cf, "present_value": round(pv_cf, 2)})

    # PV of residual value
    if residual_value and residual_value > 0:
        df = 1 / math.pow(1 + rate, len(cash_flows))
        pv_residual = residual_value * df
        pv_inflows += pv_residual
        details.append(
            {"year": len(cash_flows), "description": "Residual value", "present_value": round(pv_residual, 2)}
        )

    npv = pv_inflows - initial_investment

    return {
        "initial_investment": initial_investment,
        "residual_value": residual_value,
        "rate": rate,
        "present_value_details": details,
        "total_pv_inflows": round(pv_inflows, 2),
        "net_present_value": round(npv, 2),
        "decision": "Accept" if npv > 0 else "Reject" if npv < 0 else "Indifferent",
    }


@app.post("/perpetuity")
async def calculate_present_value_perpetuity(annual_cash_flow: float, rate: float):
    """Calculate PV of perpetuity (infinite cash flows)."""
    if rate <= 0:
        return {"error": "Rate must be positive for perpetuity"}
    pv = annual_cash_flow / rate
    return {
        "annual_cash_flow": annual_cash_flow,
        "rate": rate,
        "present_value_perpetuity": round(pv, 2),
        "formula": f"{annual_cash_flow} / {rate} = {pv}",
    }


@app.post("/growing-perpetuity")
async def calculate_pv_growing_perpetuity(initial_cash_flow: float, growth_rate: float, discount_rate: float):
    """Calculate PV of growing perpetuity."""
    if discount_rate <= growth_rate:
        return {"error": "Discount rate must be greater than growth rate"}

    pv = initial_cash_flow / (discount_rate - growth_rate)
    return {
        "initial_cash_flow": initial_cash_flow,
        "growth_rate": growth_rate,
        "discount_rate": discount_rate,
        "present_value": round(pv, 2),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
