"""
Treasury Management Service
Port: 8187
Cash management, liquidity optimization
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Treasury Management Service", version="1.0.0")

class BankAccount(BaseModel):
    account_id: str
    account_name: str
    balance: float
    currency: str

class TreasuryRequest(BaseModel):
    company_id: str
    accounts: List[BankAccount]
    target_cash_balance: float
    investment_rate: float

class TreasuryResponse(BaseModel):
    company_id: str
    total_cash: float
    surplus_cash: float
    deficit_cash: float
    recommended_investment: float
    recommended_borrowing: float

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
    return {"status": "healthy", "service": "treasury-management", "version": "1.0.0"}

@app.post("/optimize", response_model=TreasuryResponse)
async def optimize_treasury(request: TreasuryRequest):
    logger.info("Optimizing treasury", company=request.company_id)

    total_cash = sum(a.balance for a in request.accounts)
    target = request.target_cash_balance * len(request.accounts)

    surplus = max(0, total_cash - target)
    deficit = max(0, target - total_cash)

    return TreasuryResponse(
        company_id=request.company_id,
        total_cash=round(total_cash, 2),
        surplus_cash=round(surplus, 2),
        deficit_cash=round(deficit, 2),
        recommended_investment=round(surplus * 0.8, 2),
        recommended_borrowing=round(deficit * 1.1, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8187)
