"""
Regulatory Ratio Service
Port: 8229
Banking and insurance regulatory ratios
"""

import importlib.util
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
_spec = importlib.util.spec_from_file_location("regulatory_ratio_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["regulatory_ratio_calc"] = calc
_spec.loader.exec_module(calc)

app = FastAPI(title="Regulatory Ratio Service", version="1.0.0")


class RegulatoryRatio(BaseModel):
    ratio_name: str
    actual_value: float
    minimum_required: float
    buffer: float
    compliant: bool


class RegulatoryRatioRequest(BaseModel):
    company_id: str
    regulator_type: str
    tier1_capital: float
    tier2_capital: float
    risk_weighted_assets: float
    total_capital: float
    total_assets: float
    liquid_assets: float
    net_loan: float
    regulatory_capital: Dict[str, float]


class RegulatoryRatioResponse(BaseModel):
    company_id: str
    regulator_type: str
    capital_ratios: List[RegulatoryRatio]
    liquidity_ratios: List[RegulatoryRatio]
    leverage_ratios: List[RegulatoryRatio]
    overall_compliance: str
    recommendations: List[str]


async def call_internal_service(service_url: str, endpoint: str, data: dict = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            response = await client.post(url, json=data) if data else await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}


@app.get("/")
async def health_check():
    return {
        "status": "healthy",
        "service": "regulatory-ratio",
        "version": "1.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/calculate", response_model=RegulatoryRatioResponse)
async def calculate_regulatory_ratios(request: RegulatoryRatioRequest):
    logger.info("Calculating regulatory ratios", company=request.company_id, regulator=request.regulator_type)
    result = calc.regulatory_ratios(
        request.company_id,
        request.regulator_type,
        request.tier1_capital,
        request.tier2_capital,
        request.risk_weighted_assets,
        request.total_capital,
        request.total_assets,
        request.liquid_assets,
        request.net_loan,
    )
    return RegulatoryRatioResponse(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8229)
