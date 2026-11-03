"""
Financial Distress Prediction Service
Port: 8169
Altman Z-score, working capital to total assets ratio, distress prediction
"""

import importlib.util
import os
import sys
from typing import Any, Dict, Optional

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

logger = structlog.get_logger()
# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("distress_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["distress_calc"] = calc
_spec.loader.exec_module(calc)

app = FastAPI(title="Financial Distress Prediction Service", version="1.0.0")


class CompanyData(BaseModel):
    company_id: str
    company_name: str
    industry: str
    working_capital: float
    total_assets: float
    ebit: float
    total_equity: float
    total_liabilities: float
    revenue: float
    retained_earnings: float
    market_cap: float
    current_assets: float
    current_liabilities: float


class DistressPredictionRequest(BaseModel):
    company: CompanyData
    model: str = "altman"  # "altman", "springate", "fulmer"


class DistressPredictionResponse(BaseModel):
    company_id: str
    z_score: float
    z1: float
    z2: float
    z3: float
    z4: float
    z5: float
    zone: str
    probability_distress: float
    years_to_distress: float
    recommendation: str


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
    return {
        "status": "healthy",
        "service": "financial-distress-prediction",
        "version": "1.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/predict", response_model=DistressPredictionResponse)
async def predict_financial_distress(request: DistressPredictionRequest):
    logger.info("Predicting financial distress", company=request.company.company_name)
    result = calc.predict_distress(request.company.model_dump(mode="json"))
    return DistressPredictionResponse(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8169)
