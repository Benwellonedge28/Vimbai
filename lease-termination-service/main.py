"""
Lease Termination Service
Port: 8224
Lease modification and termination accounting
"""
import httpx
import structlog
from typing import Any, Dict
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Lease Termination Service", version="1.0.0")

class LeaseTerminationRequest(BaseModel):
    company_id: str
    lease_id: str
    original_lease_term: int
    remaining_term: int
    rou_asset_book_value: float
    lease_liability_book_value: float
    early_termination_penalty: float
    new_lease_payment: float
    remeasurement_required: bool

class LeaseTerminationResponse(BaseModel):
    company_id: str
    lease_id: str
    derecognition_amount: float
    gain_loss_on_termination: float
    new_lease_liability: float
    new_rou_asset: float
    right_of_use_adjustment: float
    accounting_treatment: str
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
    return {"status": "healthy", "service": "lease-termination", "version": "1.0.0"}

@app.post("/calculate", response_model=LeaseTerminationResponse)
async def calculate_lease_termination(request: LeaseTerminationRequest):
    logger.info("Calculating lease termination", company=request.company_id, lease=request.lease_id)

    derecognition = min(request.rou_asset_book_value, request.lease_liability_book_value)
    termination_cost = request.early_termination_penalty
    gain_loss = derecognition - request.lease_liability_book_value - termination_cost

    if request.new_lease_payment > 0:
        new_liability = request.new_lease_payment * request.remaining_term
        new_asset = new_liability
    else:
        new_liability = 0.0
        new_asset = 0.0

    treatment = "remeasure lease liability" if request.remeasurement_required else "derecognize ROU asset and liability"

    return LeaseTerminationResponse(
        company_id=request.company_id,
        lease_id=request.lease_id,
        derecognition_amount=round(derecognition, 2),
        gain_loss_on_termination=round(gain_loss, 2),
        new_lease_liability=round(new_liability, 2),
        new_rou_asset=round(new_asset, 2),
        right_of_use_adjustment=round(request.rou_asset_book_value - derecognition, 2),
        accounting_treatment=treatment,
        recommendations=["Document termination rationale", "Obtain lessor confirmation", "Update lease register"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8224)
