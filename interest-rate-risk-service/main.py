"""
Interest Rate Risk Service
Port: 8170
Interest rate sensitivity, gap analysis, duration matching
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Interest Rate Risk Service", version="1.0.0")

class RateSensitiveItem(BaseModel):
    item_id: str
    description: str
    balance: float
    rate_type: str  # "fixed", "variable"
    maturity_date: str
    repricing_bucket: str

class InterestRateRiskRequest(BaseModel):
    company_id: str
    reporting_date: str
    rate_sensitive_assets: List[RateSensitiveItem]
    rate_sensitive_liabilities: List[RateSensitiveItem]
    current_rate_shock: float = 2.0

class GapAnalysis(BaseModel):
    bucket: str
    asset_gap: float
    liability_gap: float
    net_gap: float
    cumulative_gap: float

class InterestRateRiskResponse(BaseModel):
    company_id: str
    total_sensitive_assets: float
    total_sensitive_liabilities: float
    gap_analysis: List[GapAnalysis]
    rate_sensitivity_nii: float
    economic_value_change: float
    duration_gap: float
    earnings_at_risk: float
    risk_rating: str

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
    return {"status": "healthy", "service": "interest-rate-risk", "version": "1.0.0"}

@app.post("/analyze", response_model=InterestRateRiskResponse)
async def analyze_interest_rate_risk(request: InterestRateRiskRequest):
    logger.info("Analyzing interest rate risk", company=request.company_id)

    total_assets = sum(item.balance for item in request.rate_sensitive_assets)
    total_liabilities = sum(item.balance for item in request.rate_sensitive_liabilities)

    buckets = ["0-3m", "3-6m", "6-12m", "1-2y", "2-5y", "5y+"]
    gaps = []
    cumulative = 0.0

    for bucket in buckets:
        asset_gap = sum(item.balance for item in request.rate_sensitive_assets if item.repricing_bucket == bucket)
        liability_gap = sum(item.balance for item in request.rate_sensitive_liabilities if item.repricing_bucket == bucket)
        net_gap = asset_gap - liability_gap
        cumulative += net_gap

        gaps.append(GapAnalysis(
            bucket=bucket,
            asset_gap=asset_gap,
            liability_gap=liability_gap,
            net_gap=net_gap,
            cumulative_gap=cumulative
        ))

    rate_shock = request.current_rate_shock / 100
    nii_impact = cumulative * rate_shock
    economic_value = total_assets * rate_shock * 0.5
    duration_gap = 1.5  # Simplified
    ear = total_liabilities * rate_shock * 0.1

    risk_rating = "LOW" if abs(nii_impact / total_assets) < 0.05 else "MEDIUM" if abs(nii_impact / total_assets) < 0.10 else "HIGH"

    return InterestRateRiskResponse(
        company_id=request.company_id,
        total_sensitive_assets=total_assets,
        total_sensitive_liabilities=total_liabilities,
        gap_analysis=gaps,
        rate_sensitivity_nii=nii_impact,
        economic_value_change=economic_value,
        duration_gap=duration_gap,
        earnings_at_risk=ear,
        risk_rating=risk_rating
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8170)
