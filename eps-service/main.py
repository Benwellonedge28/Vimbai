"""
Earnings Per Share Service
Port: 8207
Basic and diluted EPS calculations per IAS 33
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Earnings Per Share Service", version="1.0.0")


class ShareOption(BaseModel):
    option_id: str
    number_of_options: int
    exercise_price: float
    average_market_price: float
    dilution_effect: int


class EPSRequest(BaseModel):
    company_id: str
    period: str
    net_profit_attributable: float
    weighted_average_shares: int
    potential_dilutive_shares: List[Dict[str, Any]]
    anti_dilutive_items: List[Dict[str, Any]]
    discontinued_operations_profit: float


class EPSResponse(BaseModel):
    company_id: str
    period: str
    basic_eps: float
    diluted_eps: float
    eps_from_continuing_operations: float
    weighted_average_shares: int
    diluted_shares: int
    dilutive_effect: int
    anti_dilutive_effect: int
    potential_dilutive_instruments: List[ShareOption]


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
    return {"status": "healthy", "service": "eps", "version": "1.0.0"}


@app.post("/calculate", response_model=EPSResponse)
async def calculate_eps(request: EPSRequest):
    logger.info("Calculating EPS", company=request.company_id, period=request.period)

    basic_eps = (
        request.net_profit_attributable / request.weighted_average_shares if request.weighted_average_shares else 0
    )

    continuing_profit = request.net_profit_attributable - request.discontinued_operations_profit
    eps_continuing = continuing_profit / request.weighted_average_shares if request.weighted_average_shares else 0

    dilutive_effect = 0
    diluted_shares = request.weighted_average_shares
    instruments = []

    for option in request.potential_dilutive_shares:
        exercise_price = option.get("exercise_price", 0)
        market_price = option.get("average_market_price", 0)
        num_options = option.get("number_of_options", 0)

        if market_price > exercise_price:
            treasury_shares = int(num_options * (1 - exercise_price / market_price))
            dilutive_effect += treasury_shares
            instruments.append(
                ShareOption(
                    option_id=option.get("id", ""),
                    number_of_options=num_options,
                    exercise_price=exercise_price,
                    average_market_price=market_price,
                    dilution_effect=treasury_shares,
                )
            )

    for item in request.anti_dilutive_items:
        dilutive_effect -= item.get("anti_dilutive_shares", 0)

    diluted_shares += max(0, dilutive_effect)
    diluted_eps = request.net_profit_attributable / diluted_shares if diluted_shares else 0

    return EPSResponse(
        company_id=request.company_id,
        period=request.period,
        basic_eps=round(basic_eps, 4),
        diluted_eps=round(diluted_eps, 4),
        eps_from_continuing_operations=round(eps_continuing, 4),
        weighted_average_shares=request.weighted_average_shares,
        diluted_shares=diluted_shares,
        dilutive_effect=max(0, dilutive_effect),
        anti_dilutive_effect=max(0, abs(sum(i.get("anti_dilutive_shares", 0) for i in request.anti_dilutive_items))),
        potential_dilutive_instruments=instruments,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8207)
