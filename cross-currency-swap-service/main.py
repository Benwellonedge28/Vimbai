"""
Cross Currency Swap Service
Port: 8256
Cross-currency swap valuation and analysis
"""

import math
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Cross Currency Swap Service", version="1.0.0")


class CrossCurrencySwap(BaseModel):
    swap_id: str
    notional_1: float
    currency_1: str
    rate_1: float
    notional_2: float
    currency_2: str
    rate_2: float
    tenor_years: float
    exchange_at_inception: bool


class CrossCurrencySwapRequest(BaseModel):
    company_id: str
    swaps: List[CrossCurrencySwap]
    fx_spot: Dict[str, float]
    ois_rates: Dict[str, float]


class CrossCurrencySwapResponse(BaseModel):
    company_id: str
    swap_valuations: List[Dict[str, Any]]
    currency_exposure: Dict[str, Any]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "cross-currency-swap", "version": "1.0.0"}


@app.post("/value", response_model=CrossCurrencySwapResponse)
async def value_cross_currency_swaps(request: CrossCurrencySwapRequest):
    logger.info("Valuing cross-currency swaps", company=request.company_id)

    swap_valuations = []
    total_mtm = 0
    exposure_by_currency = {}

    for swap in request.swaps:
        rate_1 = request.ois_rates.get(swap.currency_1, swap.rate_1)
        rate_2 = request.ois_rates.get(swap.currency_2, swap.rate_2)
        fx = request.fx_spot.get(f"{swap.currency_1}/{swap.currency_2}", 1)

        pv_leg_1 = swap.notional_1 * math.exp(-rate_1 * swap.tenor_years)
        pv_leg_2 = swap.notional_2 * math.exp(-rate_2 * swap.tenor_years)

        pv_interest_1 = (
            swap.notional_1 * swap.rate_1 * sum(math.exp(-rate_1 * t) for t in [1, 2, 3, 4, 5] if t <= swap.tenor_years)
            if swap.tenor_years <= 5
            else swap.notional_1 * swap.rate_1 / rate_1 * (1 - math.exp(-rate_1 * swap.tenor_years))
        )
        pv_interest_2 = (
            swap.notional_2 * swap.rate_2 * sum(math.exp(-rate_2 * t) for t in [1, 2, 3, 4, 5] if t <= swap.tenor_years)
            if swap.tenor_years <= 5
            else swap.notional_2 * swap.rate_2 / rate_2 * (1 - math.exp(-rate_2 * swap.tenor_years))
        )

        mtm_1 = pv_leg_1 + pv_interest_1
        mtm_2 = (pv_leg_2 + pv_interest_2) * fx

        net_mtm = mtm_1 - mtm_2

        swap_valuations.append(
            {
                "swap_id": swap.swap_id,
                "notional_1": swap.notional_1,
                "currency_1": swap.currency_1,
                "notional_2": swap.notional_2,
                "currency_2": swap.currency_2,
                "rate_1": swap.rate_1,
                "rate_2": swap.rate_2,
                "tenor_years": swap.tenor_years,
                "pv_leg_1": round(mtm_1, 2),
                "pv_leg_2": round(mtm_2, 2),
                "mtm": round(net_mtm, 2),
                "mtm_currency1": round(net_mtm, 2),
                "cross_currency_basis": round((rate_2 - rate_1) * 100, 4),
            }
        )

        total_mtm += net_mtm

        exposure_by_currency[swap.currency_1] = exposure_by_currency.get(swap.currency_1, 0) + swap.notional_1
        exposure_by_currency[swap.currency_2] = exposure_by_currency.get(swap.currency_2, 0) - swap.notional_2

    recommendations = []
    if abs(total_mtm) > 1000000:
        recommendations.append("Significant MTM - review counterparty exposure")
    if len(exposure_by_currency) > 3:
        recommendations.append("Complex multi-currency exposure - simplify where possible")

    return CrossCurrencySwapResponse(
        company_id=request.company_id,
        swap_valuations=swap_valuations,
        currency_exposure={
            "total_mtm": round(total_mtm, 2),
            "by_currency": {k: round(v, 2) for k, v in exposure_by_currency.items()},
        },
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8256)
