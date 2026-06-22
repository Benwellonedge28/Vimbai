"""
Business Combination Service
Port: 8225
M&A accounting under IFRS 3
"""
import httpx
import structlog
from typing import Any, Dict
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Business Combination Service", version="1.0.0")

class BusinessCombinationRequest(BaseModel):
    company_id: str
    target_id: str
    consideration_transferred: float
    fair_value_of_identifiable_assets: float
    fair_value_of_identifiable_liabilities: float
    fair_value_of_non_controlling_interests: float
    acquisition_date: str
    goodwill_recognized: float

class BusinessCombinationResponse(BaseModel):
    company_id: str
    target_id: str
    consideration_paid: float
    fair_value_of_net_assets: float
    goodwill: float
    bargain_purchase_gain: float
    non_controlling_interests: float
    purchase_price_allocation: Dict[str, float]
    recommendations: list

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
    return {"status": "healthy", "service": "business-combination", "version": "1.0.0"}

@app.post("/calculate", response_model=BusinessCombinationResponse)
async def calculate_business_combination(request: BusinessCombinationRequest):
    logger.info("Calculating business combination", company=request.company_id, target=request.target_id)

    net_assets = request.fair_value_of_identifiable_assets - request.fair_value_of_identifiable_liabilities

    goodwill_calc = (
        request.consideration_transferred +
        request.fair_value_of_non_controlling_interests -
        net_assets
    )

    bargain_gain = 0.0
    if goodwill_calc < 0:
        bargain_gain = abs(goodwill_calc)
        goodwill_calc = 0.0

    ppa = {
        "intangible_assets": request.fair_value_of_identifiable_assets * 0.3,
        "tangible_assets": request.fair_value_of_identifiable_assets * 0.7,
        "liabilities_assumed": request.fair_value_of_identifiable_liabilities
    }

    return BusinessCombinationResponse(
        company_id=request.company_id,
        target_id=request.target_id,
        consideration_paid=request.consideration_transferred,
        fair_value_of_net_assets=round(net_assets, 2),
        goodwill=round(goodwill_calc, 2),
        bargain_purchase_gain=round(bargain_gain, 2),
        non_controlling_interests=request.fair_value_of_non_controlling_interests,
        purchase_price_allocation={k: round(v, 2) for k, v in ppa.items()},
        recommendations=["Complete purchase price allocation within 12 months", "Identify all intangible assets", "Obtain independent valuations"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8225)
