"""
Liquidity Risk Service
Port: 8164
Liquidity coverage ratio, net stable funding ratio, cash flow matching
"""

import importlib.util
import os
import sys
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field

# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("liquidity_risk_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["liquidity_risk_calc"] = calc
_spec.loader.exec_module(calc)

logger = structlog.get_logger()
app = FastAPI(title="Liquidity Risk Service", version="1.0.0")


class LiquidityAsset(BaseModel):
    asset_id: str
    asset_type: str
    value: float
    haircut: float = 0.0
    maturity_bucket: str


class Liability(BaseModel):
    liability_id: str
    liability_type: str
    value: float
    outflow_rate: float
    maturity_bucket: str


class LiquidityAnalysisRequest(BaseModel):
    company_id: str
    reporting_date: str
    liquid_assets: List[LiquidityAsset]
    liabilities: List[Liability]
    cash_flows: Dict[str, List[float]]


class LiquidityAnalysisResponse(BaseModel):
    company_id: str
    lcr_ratio: float
    nsfr_ratio: float
    liquid_asset_buffer: float
    net_cash_position: float
    maturity_mismatch: Dict[str, float]
    survival_horizon_days: int
    liquidity_risk_rating: str


async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
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
    return {"status": "healthy", "service": "liquidity-risk", "version": "1.0.0"}


def _reference(payload):
    """The original Python calculation, kept as the oracle."""
    liquid_assets = sum(a["value"] * (1 - a["haircut"]) for a in payload["liquid_assets"])
    total_outflows = sum(l["value"] * l["outflow_rate"] for l in payload["liabilities"])
    net_inflows = sum(l["value"] for l in payload["liabilities"]) * 0.1

    lcr = (
        (liquid_assets / (total_outflows - net_inflows * 0.75)) * 100 if total_outflows > net_inflows else float("inf")
    )
    lcr_ratio = min(500, lcr)

    asf = sum(l["value"] for l in payload["liabilities"]) * 0.95
    rsf = sum(a["value"] for a in payload["liquid_assets"]) * 0.5
    nsfr = (asf / rsf) * 100 if rsf > 0 else 0

    survival = int(liquid_assets / (total_outflows / 30))

    rating = "LOW" if lcr_ratio >= 150 else "MEDIUM" if lcr_ratio >= 100 else "HIGH"

    return dict(
        company_id=payload["company_id"],
        lcr_ratio=lcr_ratio,
        nsfr_ratio=nsfr,
        liquid_asset_buffer=liquid_assets,
        net_cash_position=liquid_assets - total_outflows,
        maturity_mismatch={"30_day": -50000, "90_day": -200000, "1_year": -100000},
        survival_horizon_days=survival,
        liquidity_risk_rating=rating,
    )


@app.post("/analyze", response_model=LiquidityAnalysisResponse)
async def analyze_liquidity(request: LiquidityAnalysisRequest):
    logger.info("Calculating", company=getattr(request, "company_id", ""))
    result = calc.analyze(request.model_dump(), _reference)
    return LiquidityAnalysisResponse(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8164)
