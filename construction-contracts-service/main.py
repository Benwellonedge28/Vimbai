"""
Construction Contracts Service
Port: 8219
Percentage of completion and completed contract methods
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Construction Contracts Service", version="1.0.0")

class ContractAnalysis(BaseModel):
    contract_id: str
    contract_value: float
    costs_incurred: float
    estimated_total_costs: float
    progress_percentage: float
    revenue_to_date: float
    recognized_profit: float
    expected_profit: float
    provision_for_losses: float

class ConstructionContractRequest(BaseModel):
    company_id: str
    period: str
    contracts: List[Dict[str, Any]]
    accounting_method: str

class ConstructionContractResponse(BaseModel):
    company_id: str
    period: str
    contract_analysis: List[ContractAnalysis]
    total_contract_value: float
    total_revenue_recognized: float
    total_provision_for_losses: float
    completed_contracts: int
    ongoing_contracts: int
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
    return {"status": "healthy", "service": "construction-contracts", "version": "1.0.0"}

@app.post("/analyze", response_model=ConstructionContractResponse)
async def analyze_construction_contracts(request: ConstructionContractRequest):
    logger.info("Analyzing construction contracts", company=request.company_id, period=request.period)

    contract_analysis = []
    total_value = 0.0
    total_revenue = 0.0
    total_provision = 0.0
    completed = 0
    ongoing = 0

    for contract in request.contracts:
        value = contract.get("contract_value", 0)
        costs = contract.get("costs_incurred", 0)
        etc = contract.get("estimated_total_costs", costs)
        progress = costs / etc if etc else 0
        revenue = value * progress
        expected_profit = value - etc
        provision = abs(expected_profit) if expected_profit < 0 else 0

        total_value += value
        total_revenue += revenue
        total_provision += provision

        if contract.get("completed", False):
            completed += 1
        else:
            ongoing += 1

        contract_analysis.append(ContractAnalysis(
            contract_id=contract.get("id", ""),
            contract_value=value,
            costs_incurred=costs,
            estimated_total_costs=etc,
            progress_percentage=round(progress * 100, 2),
            revenue_to_date=round(revenue, 2),
            recognized_profit=round(revenue - costs if revenue > costs else 0, 2),
            expected_profit=round(expected_profit, 2),
            provision_for_losses=round(provision, 2)
        ))

    return ConstructionContractResponse(
        company_id=request.company_id,
        period=request.period,
        contract_analysis=contract_analysis,
        total_contract_value=round(total_value, 2),
        total_revenue_recognized=round(total_revenue, 2),
        total_provision_for_losses=round(total_provision, 2),
        completed_contracts=completed,
        ongoing_contracts=ongoing,
        recommendations=[
            "Review cost estimates for contracts with low progress",
            "Recognize losses immediately when identified",
            "Ensure percentage of completion calculations are accurate"
        ]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8219)
