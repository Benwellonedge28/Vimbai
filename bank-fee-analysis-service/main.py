"""
Bank Fee Analysis Service
Port: 8189
Bank fee optimization, service charge analysis
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Bank Fee Analysis Service", version="1.0.0")

class BankFee(BaseModel):
    fee_id: str
    fee_type: str
    amount: float
    frequency: str

class BankFeeAnalysisRequest(BaseModel):
    company_id: str
    fees: List[BankFee]
    total_banking_volume: float

class BankFeeAnalysisResponse(BaseModel):
    company_id: str
    total_fees: float
    fee_per_transaction: float
    fees_as_percentage_volume: float
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
    return {"status": "healthy", "service": "bank-fee-analysis", "version": "1.0.0"}

@app.post("/analyze", response_model=BankFeeAnalysisResponse)
async def analyze_bank_fees(request: BankFeeAnalysisRequest):
    logger.info("Analyzing bank fees", company=request.company_id)

    total_fees = sum(f.amount for f in request.fees)
    fee_pct = (total_fees / request.total_banking_volume) * 100 if request.total_banking_volume else 0

    recommendations = []
    if fee_pct > 0.5:
        recommendations.append("Consider renegotiating banking terms")
        recommendations.append("Request fee benchmarking from other banks")
    else:
        recommendations.append("Banking costs are within acceptable range")

    return BankFeeAnalysisResponse(
        company_id=request.company_id,
        total_fees=round(total_fees, 2),
        fee_per_transaction=round(total_fees / 1000, 2),
        fees_as_percentage_volume=round(fee_pct, 2),
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8189)
