"""
Share Options Service
Port: 8223
Share-based payment accounting under IFRS 2
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Share Options Service", version="1.0.0")

class OptionGrant(BaseModel):
    grant_id: str
    number_of_options: int
    fair_value_per_option: float
    vesting_period: int
    exercise_price: float
    total_compensation_cost: float
    vesting_probability: float

class ShareOptionsRequest(BaseModel):
    company_id: str
    period: str
    option_grants: List[Dict[str, Any]]
    share_price: float

class ShareOptionsResponse(BaseModel):
    company_id: str
    period: str
    option_grants: List[OptionGrant]
    total_compensation_cost: float
    equity_settled_expense: float
    cash_settled_expense: float
    dilutive_effect: int
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
    return {"status": "healthy", "service": "share-options", "version": "1.0.0"}

@app.post("/analyze", response_model=ShareOptionsResponse)
async def analyze_share_options(request: ShareOptionsRequest):
    logger.info("Analyzing share options", company=request.company_id, period=request.period)

    option_grants = []
    total_cost = 0.0
    equity_expense = 0.0
    cash_expense = 0.0
    dilutive = 0

    for grant in request.option_grants:
        num_options = grant.get("number_of_options", 0)
        fair_value = grant.get("fair_value", 0)
        vesting_period = grant.get("vesting_period", 1)
        vesting_prob = grant.get("vesting_probability", 1.0)
        is_cash_settled = grant.get("cash_settled", False)

        total_grant_cost = num_options * fair_value
        vesting_factor = total_grant_cost / vesting_period * vesting_prob

        total_cost += total_grant_cost

        if is_cash_settled:
            cash_expense += vesting_factor
        else:
            equity_expense += vesting_factor
            if request.share_price > grant.get("exercise_price", 0):
                dilutive += num_options

        option_grants.append(OptionGrant(
            grant_id=grant.get("id", ""),
            number_of_options=num_options,
            fair_value_per_option=fair_value,
            vesting_period=vesting_period,
            exercise_price=grant.get("exercise_price", 0),
            total_compensation_cost=round(total_grant_cost, 2),
            vesting_probability=vesting_prob
        ))

    return ShareOptionsResponse(
        company_id=request.company_id,
        period=request.period,
        option_grants=option_grants,
        total_compensation_cost=round(total_cost, 2),
        equity_settled_expense=round(equity_expense, 2),
        cash_settled_expense=round(cash_expense, 2),
        dilutive_effect=dilutive,
        recommendations=["Ensure fair value calculations are documented", "Review vesting conditions", "Update option pricing model assumptions"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8223)
