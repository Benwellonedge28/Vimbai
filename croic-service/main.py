"""
Cash Return on Invested Capital Service
Port: 8214
CROIC calculation and capital efficiency
"""
import httpx
import structlog
from typing import Any, Dict
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="CROIC Service", version="1.0.0")

class CROICMetrics(BaseModel):
    free_cash_flow: float
    invested_capital: float
    cash_return_on_invested_capital: float
    croic_5_year_avg: float
    capital_efficiency_rating: str

class CROICRequest(BaseModel):
    company_id: str
    periods: list
    operating_cash_flow: float
    capital_expenditures: float
    total_assets: float
    current_liabilities: float
    tax_rate: float

class CROICResponse(BaseModel):
    company_id: str
    current_croic: CROICMetrics
    historical_croic: list
    industry_benchmark: float
    croic_vs_industry: str
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
    return {"status": "healthy", "service": "croic", "version": "1.0.0"}

@app.post("/calculate", response_model=CROICResponse)
async def calculate_croic(request: CROICRequest):
    logger.info("Calculating CROIC", company=request.company_id)

    fcf = request.operating_cash_flow - request.capital_expenditures

    invested_capital = request.total_assets - request.current_liabilities

    croic = (fcf / invested_capital) if invested_capital else 0

    capital_efficiency = "excellent" if croic > 0.15 else "good" if croic > 0.10 else "fair" if croic > 0.05 else "poor"

    historical = [{"period": p, "croic": round(croic * 0.9, 4)} for p in request.periods]
    avg_5yr = sum(h["croic"] for h in historical) / len(historical) if historical else croic

    industry_avg = 0.12
    vs_industry = "outperforming" if croic > industry_avg else "underperforming"

    return CROICResponse(
        company_id=request.company_id,
        current_croic=CROICMetrics(
            free_cash_flow=round(fcf, 2),
            invested_capital=round(invested_capital, 2),
            cash_return_on_invested_capital=round(croic, 4),
            croic_5_year_avg=round(avg_5yr, 4),
            capital_efficiency_rating=capital_efficiency
        ),
        historical_croic=historical,
        industry_benchmark=industry_avg,
        croic_vs_industry=vs_industry,
        recommendations=[
            "Maintain high CROIC through disciplined capital allocation",
            "Reinvest in high-return projects",
            "Return excess cash to shareholders if limited opportunities"
        ] if capital_efficiency in ["excellent", "good"] else [
            "Improve operational efficiency to boost FCF",
            "Review capital structure",
            "Divest underperforming assets"
        ]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8214)
