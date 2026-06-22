"""
Sustainable Growth Rate Service
Port: 8215
Internal growth rate and sustainable growth analysis
"""
import httpx
import structlog
from typing import Any, Dict
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Sustainable Growth Rate Service", version="1.0.0")

class GrowthMetrics(BaseModel):
    return_on_equity: float
    dividend_payout_ratio: float
    retention_ratio: float
    internal_growth_rate: float
    sustainable_growth_rate: float
    achievable_growth_rate: float

class GrowthRateRequest(BaseModel):
    company_id: str
    period: str
    net_income: float
    total_equity: float
    dividends_paid: float
    target_debt_to_equity: float
    actual_debt_to_equity: float

class GrowthRateResponse(BaseModel):
    company_id: str
    period: str
    growth_metrics: GrowthMetrics
    growth_comparison: Dict[str, float]
    growth_assessment: str
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
    return {"status": "healthy", "service": "sustainable-growth", "version": "1.0.0"}

@app.post("/calculate", response_model=GrowthRateResponse)
async def calculate_sustainable_growth(request: GrowthRateRequest):
    logger.info("Calculating sustainable growth rate", company=request.company_id, period=request.period)

    roe = request.net_income / request.total_equity if request.total_equity else 0

    payout_ratio = request.dividends_paid / request.net_income if request.net_income else 0

    retention_ratio = 1 - payout_ratio

    internal_gr = roe * retention_ratio

    sustainable_gr = roe * retention_ratio * (1 + request.target_debt_to_equity)

    achievable = min(request.actual_debt_to_equity / request.target_debt_to_equity, 1.0) * sustainable_gr if request.target_debt_to_equity else internal_gr

    assessment = "aggressive" if achievable > 0.2 else "moderate" if achievable > 0.1 else "conservative"

    return GrowthRateResponse(
        company_id=request.company_id,
        period=request.period,
        growth_metrics=GrowthMetrics(
            return_on_equity=round(roe, 4),
            dividend_payout_ratio=round(payout_ratio, 4),
            retention_ratio=round(retention_ratio, 4),
            internal_growth_rate=round(internal_gr, 4),
            sustainable_growth_rate=round(sustainable_gr, 4),
            achievable_growth_rate=round(achievable, 4)
        ),
        growth_comparison={
            "internal_growth": round(internal_gr * 100, 2),
            "sustainable_growth": round(sustainable_gr * 100, 2),
            "achievable_growth": round(achievable * 100, 2)
        },
        growth_assessment=assessment,
        recommendations=[
            "Growth strategy aligns with sustainable capacity" if assessment == "moderate" else "Consider adjusting growth targets",
            "Monitor ROE sustainability",
            "Balance dividend policy with growth investments"
        ]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8215)
