"""
Investment Portfolio Service
Port: 8193
Short-term investment management
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Investment Portfolio Service", version="1.0.0")

class Investment(BaseModel):
    investment_id: str
    investment_type: str
    principal: float
    maturity: str
    yield_rate: float

class InvestmentPortfolioRequest(BaseModel):
    company_id: str
    investments: List[Investment]
    target_liquidity_days: int

class InvestmentPortfolioResponse(BaseModel):
    company_id: str
    total_invested: float
    weighted_yield: float
    average_maturity_days: int
    liquidity_score: float
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
    return {"status": "healthy", "service": "investment-portfolio", "version": "1.0.0"}

@app.post("/analyze", response_model=InvestmentPortfolioResponse)
async def analyze_investment_portfolio(request: InvestmentPortfolioRequest):
    logger.info("Analyzing investment portfolio", company=request.company_id)

    total = sum(i.principal for i in request.investments)
    weighted_yield = sum(i.principal * i.yield_rate for i in request.investments) / total if total else 0
    avg_maturity = 90
    liquidity = 100 if request.target_liquidity_days >= 30 else 75

    return InvestmentPortfolioResponse(
        company_id=request.company_id,
        total_invested=round(total, 2),
        weighted_yield=round(weighted_yield, 2),
        average_maturity_days=avg_maturity,
        liquidity_score=liquidity,
        recommendations=["Consider laddering maturities for better liquidity"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8193)
