"""
Foreign Exchange Service
Port: 8192
FX exposure calculation, hedging strategies
"""
import httpx
import structlog
from typing import Any, Dict
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Foreign Exchange Service", version="1.0.0")

class FXExposureRequest(BaseModel):
    company_id: str
    currency_pair: str
    exposure_amount: float
    spot_rate: float
    forward_rate: float
    volatility: float

class FXExposureResponse(BaseModel):
    company_id: str
    currency_pair: str
    exposure_value_home: float
    spot_value: float
    forward_value: float
    hedge_ratio_recommended: float
    var_95: float

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
    return {"status": "healthy", "service": "foreign-exchange", "version": "1.0.0"}

@app.post("/analyze", response_model=FXExposureResponse)
async def analyze_fx_exposure(request: FXExposureRequest):
    logger.info("Analyzing FX exposure", company=request.company_id)

    spot_value = request.exposure_amount * request.spot_rate
    forward_value = request.exposure_amount * request.forward_rate
    var_95 = request.exposure_amount * request.volatility * 1.65

    return FXExposureResponse(
        company_id=request.company_id,
        currency_pair=request.currency_pair,
        exposure_value_home=round(spot_value, 2),
        spot_value=round(spot_value, 2),
        forward_value=round(forward_value, 2),
        hedge_ratio_recommended=75.0,
        var_95=round(var_95, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8192)
