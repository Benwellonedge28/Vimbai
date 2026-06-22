"""
Transfer Pricing Service
Port: 8185
Intercompany transfer pricing methods, arm's length testing
"""
import httpx
import structlog
from typing import Any, Dict
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Transfer Pricing Service", version="1.0.0")

class TransferPricingRequest(BaseModel):
    company_id: str
    transfer_type: str
    cost: float
    market_price: float
    comparable_profit_margin: float
    transaction_volume: int

class TransferPricingResponse(BaseModel):
    company_id: str
    transfer_type: str
    recommended_method: str
    transfer_price: float
    arm_length_range_low: float
    arm_length_range_high: float
    compliance_status: str

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
    return {"status": "healthy", "service": "transfer-pricing", "version": "1.0.0"}

@app.post("/calculate", response_model=TransferPricingResponse)
async def calculate_transfer_price(request: TransferPricingRequest):
    logger.info("Calculating transfer price", company=request.company_id)

    cost_plus_price = request.cost * (1 + request.comparable_profit_margin / 100)
    arm_length_low = request.market_price * 0.9
    arm_length_high = request.market_price * 1.1

    if request.transfer_type == "goods":
        method = "Comparable Uncontrolled Price"
        transfer_price = request.market_price
    else:
        method = "Cost Plus"
        transfer_price = cost_plus_price

    compliant = arm_length_low <= transfer_price <= arm_length_high

    return TransferPricingResponse(
        company_id=request.company_id,
        transfer_type=request.transfer_type,
        recommended_method=method,
        transfer_price=round(transfer_price, 2),
        arm_length_range_low=round(arm_length_low, 2),
        arm_length_range_high=round(arm_length_high, 2),
        compliance_status="Compliant" if compliant else "Review Required"
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8185)
