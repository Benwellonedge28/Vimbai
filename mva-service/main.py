"""
Market Value Added Service
Port: 8213
MVA calculation and market performance analysis
"""

from typing import Any, Dict

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Market Value Added Service", version="1.0.0")


class MVAMetrics(BaseModel):
    market_capitalization: float
    invested_capital: float
    market_value_added: float
    mva_ratio: float
    market_to_book_ratio: float
    q_ratio: float


class MVARequest(BaseModel):
    company_id: str
    period: str
    share_price: float
    shares_outstanding: float
    total_assets: float
    current_liabilities: float
    equity: float
    intangibles_book_value: float


class MVAResponse(BaseModel):
    company_id: str
    period: str
    mva_metrics: MVAMetrics
    shareholder_value_analysis: Dict[str, Any]
    premium_discount: str
    investment_signal: str
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
    return {"status": "healthy", "service": "mva", "version": "1.0.0"}


@app.post("/calculate", response_model=MVAResponse)
async def calculate_mva(request: MVARequest):
    logger.info("Calculating MVA", company=request.company_id, period=request.period)

    market_cap = request.share_price * request.shares_outstanding

    invested_capital = request.total_assets - request.current_liabilities

    mva = market_cap - invested_capital

    mva_ratio = market_cap / invested_capital if invested_capital else 0

    market_to_book = market_cap / request.equity if request.equity else 0

    q_ratio = (market_cap + request.intangibles_book_value) / request.total_assets if request.total_assets else 0

    premium_discount = "premium" if mva > 0 else "discount"

    investment_signal = "buy" if mva_ratio > 1.5 else "hold" if mva_ratio > 1.0 else "sell"

    return MVAResponse(
        company_id=request.company_id,
        period=request.period,
        mva_metrics=MVAMetrics(
            market_capitalization=round(market_cap, 2),
            invested_capital=round(invested_capital, 2),
            market_value_added=round(mva, 2),
            mva_ratio=round(mva_ratio, 2),
            market_to_book_ratio=round(market_to_book, 2),
            q_ratio=round(q_ratio, 2),
        ),
        shareholder_value_analysis={
            "value_created_for_shareholders": round(mva, 2),
            "return_on_market_value": round((mva / invested_capital) * 100, 2) if invested_capital else 0,
            "intangible_value_recognized": round(market_cap - request.equity, 2),
        },
        premium_discount=premium_discount,
        investment_signal=investment_signal,
        recommendations=(
            [
                "Market values company at a premium - growth expectations high",
                "Focus on sustaining intangible value creation",
                "Communicate value drivers to market",
            ]
            if premium_discount == "premium"
            else [
                "Market perceives value destruction",
                "Review strategic direction and operational efficiency",
                "Consider restructuring to unlock shareholder value",
            ]
        ),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8213)
