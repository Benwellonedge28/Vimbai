"""
Trade Finance Service
Port: 8191
Letters of credit, bank guarantees
"""
import httpx
import structlog
from typing import Any, Dict
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Trade Finance Service", version="1.0.0")

class TradeFinanceRequest(BaseModel):
    company_id: str
    transaction_value: float
    lc_type: str
    tenor_days: int
    issuing_bank_rating: str

class TradeFinanceResponse(BaseModel):
    company_id: str
    lc_type: str
    commission_rate: float
    commission_amount: float
    advisory_fee: float
    total_cost: float

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
    return {"status": "healthy", "service": "trade-finance", "version": "1.0.0"}

@app.post("/quote", response_model=TradeFinanceResponse)
async def quote_trade_finance(request: TradeFinanceRequest):
    logger.info("Quoting trade finance", company=request.company_id)

    base_rate = 0.015
    tenor_factor = request.tenor_days / 365
    commission = request.transaction_value * base_rate * tenor_factor
    advisory_fee = request.transaction_value * 0.001

    return TradeFinanceResponse(
        company_id=request.company_id,
        lc_type=request.lc_type,
        commission_rate=round(base_rate * 100, 2),
        commission_amount=round(commission, 2),
        advisory_fee=round(advisory_fee, 2),
        total_cost=round(commission + advisory_fee, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8191)
