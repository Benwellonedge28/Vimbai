"""
Disposal Group Service
Port: 8226
Assets held for sale under IFRS 5
"""
import httpx
import structlog
from typing import Any, Dict
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Disposal Group Service", version="1.0.0")

class DisposalGroupRequest(BaseModel):
    company_id: str
    disposal_group_id: str
    total_assets: float
    total_liabilities: float
    carrying_amount: float
    fair_value_less_costs_to_sell: float
    expected_sale_date: str
    classification_date: str

class DisposalGroupResponse(BaseModel):
    company_id: str
    disposal_group_id: str
    impairment_loss: float
    remeasurement_gain: float
    assets_held_for_sale: float
    liabilities_held_for_sale: float
    discontinued_operations_classification: bool
    reclassification_adjustments: float
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
    return {"status": "healthy", "service": "disposal-group", "version": "1.0.0"}

@app.post("/analyze", response_model=DisposalGroupResponse)
async def analyze_disposal_group(request: DisposalGroupRequest):
    logger.info("Analyzing disposal group", company=request.company_id, group=request.disposal_group_id)

    fvlcts = request.fair_value_less_costs_to_sell

    if fvlcts < request.carrying_amount:
        impairment = request.carrying_amount - fvlcts
        remeasurement_gain = 0.0
    else:
        impairment = 0.0
        remeasurement_gain = fvlcts - request.carrying_amount

    discontinued = request.expected_sale_date and (request.total_liabilities / request.total_assets) < 0.5

    return DisposalGroupResponse(
        company_id=request.company_id,
        disposal_group_id=request.disposal_group_id,
        impairment_loss=round(impairment, 2),
        remeasurement_gain=round(remeasurement_gain, 2),
        assets_held_for_sale=round(fvlcts, 2),
        liabilities_held_for_sale=request.total_liabilities,
        discontinued_operations_classification=discontinued,
        reclassification_adjustments=round(request.total_assets - fvlcts, 2),
        recommendations=["Ensure criteria for held for sale are met", "Present discontinued operations separately", "Discontinue depreciation of assets"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8226)
