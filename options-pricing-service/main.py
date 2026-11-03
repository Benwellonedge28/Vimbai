"""
Options Pricing Service
Port: 8248
Options valuation and Greeks calculation
"""

import importlib.util
import math
import os
import sys
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("options_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["options_calc"] = calc
_spec.loader.exec_module(calc)

app = FastAPI(title="Options Pricing Service", version="1.0.0")


class Option(BaseModel):
    option_type: str
    strike_price: float
    spot_price: float
    time_to_expiry: float
    volatility: float
    risk_free_rate: float
    notional: float


class OptionsPricingRequest(BaseModel):
    company_id: str
    options: List[Option]
    current_spot: float


class OptionsPricingResponse(BaseModel):
    company_id: str
    option_valuations: List[Dict[str, Any]]
    portfolio_summary: Dict[str, Any]
    greeks_summary: Dict[str, float]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {
        "status": "healthy",
        "service": "options-pricing",
        "version": "1.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/price", response_model=OptionsPricingResponse)
async def price_options(request: OptionsPricingRequest):
    logger.info("Pricing options", company=request.company_id)
    result = calc.price(
        request.company_id,
        [o.model_dump() for o in request.options],
        request.current_spot,
    )
    return OptionsPricingResponse(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8248)
