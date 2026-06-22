"""
Cash Budget Service
Port: 8171
Cash receipts and payments budget, financing requirements
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Cash Budget Service", version="1.0.0")

class CashBudgetItem(BaseModel):
    item_id: str
    category: str
    description: str
    amount: float
    timing: str

class CashBudgetRequest(BaseModel):
    company_id: str
    budget_period: str
    opening_cash: float
    minimum_cash_balance: float
    expected_receipts: List[CashBudgetItem]
    expected_payments: List[CashBudgetItem]

class CashBudgetPeriod(BaseModel):
    period: str
    opening_balance: float
    receipts: float
    payments: float
    closing_balance: float
    financing: float
    cumulative_cash: float

class CashBudgetResponse(BaseModel):
    company_id: str
    budget_period: str
    periods: List[CashBudgetPeriod]
    total_receipts: float
    total_payments: float
    net_cash_flow: float
    minimum_cash_balance: float
    peak_financing_required: float
    total_financing_cost: float

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
    return {"status": "healthy", "service": "cash-budget", "version": "1.0.0"}

@app.post("/prepare", response_model=CashBudgetResponse)
async def prepare_cash_budget(request: CashBudgetRequest):
    logger.info("Preparing cash budget", company=request.company_id)

    periods = []
    opening = request.opening_cash
    cumulative = opening
    peak_financing = 0.0
    total_receipts = 0.0
    total_payments = 0.0

    for i in range(1, 13):
        receipts = sum(item.amount for item in request.expected_receipts if f"Q{(i-1)//3+1}" in item.timing)
        payments = sum(item.amount for item in request.expected_payments if f"Q{(i-1)//3+1}" in item.timing)

        closing = opening + receipts - payments

        if closing < request.minimum_cash_balance:
            financing = request.minimum_cash_balance - closing
            closing = request.minimum_cash_balance
        else:
            financing = 0

        cumulative += receipts - payments
        peak_financing = max(peak_financing, financing)

        total_receipts += receipts
        total_payments += payments

        periods.append(CashBudgetPeriod(
            period=f"Month {i}",
            opening_balance=opening,
            receipts=receipts,
            payments=payments,
            closing_balance=closing,
            financing=financing,
            cumulative_cash=cumulative
        ))

        opening = closing

    return CashBudgetResponse(
        company_id=request.company_id,
        budget_period=request.budget_period,
        periods=periods,
        total_receipts=total_receipts,
        total_payments=total_payments,
        net_cash_flow=total_receipts - total_payments,
        minimum_cash_balance=request.minimum_cash_balance,
        peak_financing_required=peak_financing,
        total_financing_cost=peak_financing * 0.05
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8171)
