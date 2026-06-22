"""
Debt Management Service
Port: 8194
Debt scheduling, refinancing analysis
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Debt Management Service", version="1.0.0")

class DebtFacility(BaseModel):
    facility_id: str
    facility_type: str
    principal: float
    interest_rate: float
    maturity_date: str

class DebtManagementRequest(BaseModel):
    company_id: str
    facilities: List[DebtFacility]
    current_ebitda: float

class DebtManagementResponse(BaseModel):
    company_id: str
    total_debt: float
    weighted_interest_rate: float
    debt_to_ebitda: float
    annual_interest_expense: float
    refinancing_opportunities: List[Dict[str, Any]]

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
    return {"status": "healthy", "service": "debt-management", "version": "1.0.0"}

@app.post("/analyze", response_model=DebtManagementResponse)
async def analyze_debt_management(request: DebtManagementRequest):
    logger.info("Analyzing debt management", company=request.company_id)

    total_debt = sum(f.principal for f in request.facilities)
    weighted_rate = sum(f.principal * f.interest_rate for f in request.facilities) / total_debt if total_debt else 0
    interest_expense = total_debt * weighted_rate

    return DebtManagementResponse(
        company_id=request.company_id,
        total_debt=round(total_debt, 2),
        weighted_interest_rate=round(weighted_rate, 4),
        debt_to_ebitda=round(total_debt / request.current_ebitda, 2) if request.current_ebitda else 0,
        annual_interest_expense=round(interest_expense, 2),
        refinancing_opportunities=[{"opportunity": "Consider refinancing if rates drop by 0.5%"}]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8194)
