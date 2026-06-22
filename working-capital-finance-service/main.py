"""
Working Capital Finance Service
Port: 8190
Invoice discounting, factoring, supply chain finance
"""
import httpx
import structlog
from typing import Any, Dict
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Working Capital Finance Service", version="1.0.0")

class WorkingCapitalFinanceRequest(BaseModel):
    company_id: str
    receivables_value: float
    facility_type: str
    advance_rate: float
    fee_percentage: float

class WorkingCapitalFinanceResponse(BaseModel):
    company_id: str
    facility_type: str
    available_funds: float
    facility_fee: float
    net_proceeds: float
    annual_cost_percentage: float

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
    return {"status": "healthy", "service": "working-capital-finance", "version": "1.0.0"}

@app.post("/calculate", response_model=WorkingCapitalFinanceResponse)
async def calculate_working_capital_finance(request: WorkingCapitalFinanceRequest):
    logger.info("Calculating working capital finance", company=request.company_id)

    available = request.receivables_value * (request.advance_rate / 100)
    fee = available * (request.fee_percentage / 100)
    net = available - fee
    apr = (fee / available) * 100 * (12 / 3) if available else 0

    return WorkingCapitalFinanceResponse(
        company_id=request.company_id,
        facility_type=request.facility_type,
        available_funds=round(available, 2),
        facility_fee=round(fee, 2),
        net_proceeds=round(net, 2),
        annual_cost_percentage=round(apr, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8190)
