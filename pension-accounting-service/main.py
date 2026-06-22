"""
Pension Accounting Service
Port: 8222
Defined benefit and contribution plans under IAS 19
"""
import httpx
import structlog
from typing import Any, Dict
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Pension Accounting Service", version="1.0.0")

class PensionMetrics(BaseModel):
    defined_benefit_obligation: float
    plan_assets: float
    net_liability: float
    current_service_cost: float
    interest_cost: float
    return_on_plan_assets: float
    actuarial_gains_losses: float
    past_service_cost: float

class PensionAccountingRequest(BaseModel):
    company_id: str
    period: str
    plan_assets: float
    defined_benefit_obligation: float
    current_service_cost: float
    interest_cost: float
    return_on_assets: float
    actuarial_losses: float
    contributions_paid: float
    benefits_paid: float

class PensionAccountingResponse(BaseModel):
    company_id: str
    period: str
    pension_metrics: PensionMetrics
    funded_status: str
    pension_expense: float
    asset_ceiling_test: bool
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
    return {"status": "healthy", "service": "pension-accounting", "version": "1.0.0"}

@app.post("/analyze", response_model=PensionAccountingResponse)
async def analyze_pension_accounting(request: PensionAccountingRequest):
    logger.info("Analyzing pension accounting", company=request.company_id, period=request.period)

    net_liability = request.defined_benefit_obligation - request.plan_assets

    pension_expense = (
        request.current_service_cost +
        request.interest_cost -
        request.return_on_assets +
        request.actuarial_losses
    )

    funded_status = "overfunded" if net_liability < 0 else "underfunded" if net_liability > 0 else "fully_funded"

    asset_ceiling = request.plan_assets * 1.1
    asset_ceiling_test = request.plan_assets <= asset_ceiling

    return PensionAccountingResponse(
        company_id=request.company_id,
        period=request.period,
        pension_metrics=PensionMetrics(
            defined_benefit_obligation=request.defined_benefit_obligation,
            plan_assets=request.plan_assets,
            net_liability=net_liability,
            current_service_cost=request.current_service_cost,
            interest_cost=request.interest_cost,
            return_on_plan_assets=request.return_on_assets,
            actuarial_gains_losses=request.actuarial_losses,
            past_service_cost=0.0
        ),
        funded_status=funded_status,
        pension_expense=round(pension_expense, 2),
        asset_ceiling_test=asset_ceiling_test,
        recommendations=["Review actuarial assumptions annually", "Monitor pension funding levels", "Consider risk-sharing arrangements"] if funded_status == "underfunded" else ["Continue monitoring pension obligations"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8222)
