"""
Currency Hedging Service
Port: 8251
FX risk management and hedging
"""

import math
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Currency Hedging Service", version="1.0.0")


class FXExposure(BaseModel):
    currency_pair: str
    exposure_amount: float
    spot_rate: float
    expected_rate: float
    volatility: float


class CurrencyHedgingRequest(BaseModel):
    company_id: str
    exposures: List[FXExposure]
    current_hedge_ratio: float
    target_hedge_ratio: float
    risk_aversion: float


class CurrencyHedgingResponse(BaseModel):
    company_id: str
    exposure_analysis: List[Dict[str, Any]]
    hedge_recommendations: List[Dict[str, Any]]
    portfolio_metrics: Dict[str, Any]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "currency-hedging", "version": "1.0.0"}


@app.post("/analyze", response_model=CurrencyHedgingResponse)
async def analyze_currency_hedge(request: CurrencyHedgingRequest):
    logger.info("Analyzing currency hedging", company=request.company_id)

    exposure_analysis = []
    total_exposure = 0
    total_hedged_value = 0

    for exp in request.exposures:
        exposure_usd = exp.exposure_amount / exp.spot_rate
        hedged_amount = exposure_usd * request.current_hedge_ratio
        hedge_cost = hedged_amount * exp.volatility * 0.1

        var_95 = exposure_usd * exp.volatility * 1.65
        hedged_var = exposure_usd * (1 - request.current_hedge_ratio) * exp.volatility * 1.65

        exposure_analysis.append(
            {
                "currency_pair": exp.currency_pair,
                "exposure_amount": exp.exposure_amount,
                "exposure_usd": round(exposure_usd, 2),
                "spot_rate": exp.spot_rate,
                "volatility": round(exp.volatility * 100, 2),
                "hedged_amount": round(hedged_amount, 2),
                "hedge_cost": round(hedge_cost, 2),
                "var_95": round(var_95, 2),
                "hedged_var": round(hedged_var, 2),
                "var_reduction": round((var_95 - hedged_var) / var_95 * 100, 2) if var_95 else 0,
            }
        )

        total_exposure += exposure_usd
        total_hedged_value += hedged_amount

    hedge_recommendations = []
    for exp in request.exposures:
        target_hedge = exp.exposure_amount / exp.spot_rate * request.target_hedge_ratio
        current_hedge = exp.exposure_amount / exp.spot_rate * request.current_hedge_ratio

        if request.target_hedge_ratio > request.current_hedge_ratio:
            hedge_recommendations.append(
                {
                    "currency_pair": exp.currency_pair,
                    "action": "BUY",
                    "amount": round(target_hedge - current_hedge, 2),
                    "instrument": "Forward",
                    "estimated_cost": round((target_hedge - current_hedge) * exp.volatility * 0.1, 2),
                }
            )
        elif request.target_hedge_ratio < request.current_hedge_ratio:
            hedge_recommendations.append(
                {
                    "currency_pair": exp.currency_pair,
                    "action": "SELL",
                    "amount": round(current_hedge - target_hedge, 2),
                    "instrument": "Forward",
                    "estimated_cost": round((current_hedge - target_hedge) * exp.volatility * 0.1, 2),
                }
            )

    portfolio_metrics = {
        "total_exposure_usd": round(total_exposure, 2),
        "hedged_value_usd": round(total_hedged_value, 2),
        "current_hedge_ratio": round(request.current_hedge_ratio * 100, 2),
        "target_hedge_ratio": round(request.target_hedge_ratio * 100, 2),
        "var_portfolio_95": round(sum(e["var_95"] for e in exposure_analysis), 2),
        "hedged_var_95": round(sum(e["hedged_var"] for e in exposure_analysis), 2),
    }

    recommendations = []
    if request.current_hedge_ratio < request.target_hedge_ratio * 0.8:
        recommendations.append("Under-hedged - increase hedge ratio to target level")
    if portfolio_metrics["hedged_var_95"] > portfolio_metrics["total_exposure_usd"] * 0.15:
        recommendations.append("VaR exceeds comfort level - consider increasing hedges")

    return CurrencyHedgingResponse(
        company_id=request.company_id,
        exposure_analysis=exposure_analysis,
        hedge_recommendations=hedge_recommendations,
        portfolio_metrics=portfolio_metrics,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8251)
