"""
Cash Pooling Service
Port: 8188
Zero balancing, notional pooling
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Cash Pooling Service", version="1.0.0")

class SubAccount(BaseModel):
    account_id: str
    balance: float
    pool_name: str

class CashPoolingRequest(BaseModel):
    company_id: str
    accounts: List[SubAccount]
    pool_type: str = "notional"

class CashPoolingResponse(BaseModel):
    company_id: str
    pool_type: str
    total_pool_balance: float
    pooled_interest_savings: float
    net_positions: List[Dict[str, Any]]

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
    return {"status": "healthy", "service": "cash-pooling", "version": "1.0.0"}

@app.post("/pool", response_model=CashPoolingResponse)
async def calculate_cash_pooling(request: CashPoolingRequest):
    logger.info("Calculating cash pooling", company=request.company_id)

    total = sum(a.balance for a in request.accounts)
    interest_rate = 0.03
    savings = abs(total) * interest_rate if total < 0 else 0

    return CashPoolingResponse(
        company_id=request.company_id,
        pool_type=request.pool_type,
        total_pool_balance=round(total, 2),
        pooled_interest_savings=round(savings, 2),
        net_positions=[{"account_id": a.account_id, "balance": a.balance} for a in request.accounts]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8188)
