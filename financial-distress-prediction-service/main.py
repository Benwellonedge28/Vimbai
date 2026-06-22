"""
Financial Distress Prediction Service
Port: 8169
Altman Z-score, working capital to total assets ratio, distress prediction
"""
import httpx
import structlog
from typing import Any, Dict, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Financial Distress Prediction Service", version="1.0.0")

class CompanyData(BaseModel):
    company_id: str
    company_name: str
    industry: str
    working_capital: float
    total_assets: float
    ebit: float
    total_equity: float
    total_liabilities: float
    revenue: float
    retained_earnings: float
    market_cap: float
    current_assets: float
    current_liabilities: float

class DistressPredictionRequest(BaseModel):
    company: CompanyData
    model: str = "altman"  # "altman", "springate", "fulmer"

class DistressPredictionResponse(BaseModel):
    company_id: str
    z_score: float
    z1: float
    z2: float
    z3: float
    z4: float
    z5: float
    zone: str
    probability_distress: float
    years_to_distress: float
    recommendation: str

async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
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
    return {"status": "healthy", "service": "financial-distress-prediction", "version": "1.0.0"}

@app.post("/predict", response_model=DistressPredictionResponse)
async def predict_financial_distress(request: DistressPredictionRequest):
    logger.info("Predicting financial distress", company=request.company.company_name)

    c = request.company

    z1 = (c.working_capital / c.total_assets) * 1.2
    z2 = (c.retained_earnings / c.total_assets) * 1.4
    z3 = (c.ebit / c.total_assets) * 3.3
    z4 = (c.total_equity / c.total_liabilities) * 0.6
    z5 = (c.revenue / c.total_assets) * 1.0

    z_score = z1 + z2 + z3 + z4 + z5

    if c.industry == "manufacturing":
        if z_score > 2.99:
            zone = "SAFE"
            prob_distress = 0.03
        elif z_score > 1.81:
            zone = "GREY"
            prob_distress = 0.35
        else:
            zone = "DISTRESS"
            prob_distress = 0.95
    else:
        if z_score > 2.6:
            zone = "SAFE"
            prob_distress = 0.05
        elif z_score > 1.5:
            zone = "GREY"
            prob_distress = 0.40
        else:
            zone = "DISTRESS"
            prob_distress = 0.90

    if z_score > 3.0:
        years = 5.0
        recommendation = "Company is financially healthy. Continue monitoring and maintain current strategies."
    elif z_score > 2.0:
        years = 2.5
        recommendation = "Company shows signs of stress. Consider reviewing operations and improving liquidity."
    elif z_score > 1.0:
        years = 1.0
        recommendation = "High risk of distress. Urgent action needed - consider restructuring options."
    else:
        years = 0.5
        recommendation = "Near-term bankruptcy risk. Seek professional financial advice immediately."

    return DistressPredictionResponse(
        company_id=c.company_id,
        z_score=round(z_score, 2),
        z1=round(z1, 2),
        z2=round(z2, 2),
        z3=round(z3, 2),
        z4=round(z4, 2),
        z5=round(z5, 2),
        zone=zone,
        probability_distress=prob_distress,
        years_to_distress=years,
        recommendation=recommendation
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8169)
