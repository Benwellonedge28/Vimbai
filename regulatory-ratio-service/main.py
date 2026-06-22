"""
Regulatory Ratio Service
Port: 8229
Banking and insurance regulatory ratios
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Regulatory Ratio Service", version="1.0.0")

class RegulatoryRatio(BaseModel):
    ratio_name: str
    actual_value: float
    minimum_required: float
    buffer: float
    compliant: bool

class RegulatoryRatioRequest(BaseModel):
    company_id: str
    regulator_type: str
    tier1_capital: float
    tier2_capital: float
    risk_weighted_assets: float
    total_capital: float
    total_assets: float
    liquid_assets: float
    net_loan: float
    regulatory_capital: Dict[str, float]

class RegulatoryRatioResponse(BaseModel):
    company_id: str
    regulator_type: str
    capital_ratios: List[RegulatoryRatio]
    liquidity_ratios: List[RegulatoryRatio]
    leverage_ratios: List[RegulatoryRatio]
    overall_compliance: str
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
    return {"status": "healthy", "service": "regulatory-ratio", "version": "1.0.0"}

@app.post("/calculate", response_model=RegulatoryRatioResponse)
async def calculate_regulatory_ratios(request: RegulatoryRatioRequest):
    logger.info("Calculating regulatory ratios", company=request.company_id, regulator=request.regulator_type)

    tier1_ratio = (request.tier1_capital / request.risk_weighted_assets) if request.risk_weighted_assets else 0
    total_cap_ratio = (request.total_capital / request.risk_weighted_assets) if request.risk_weighted_assets else 0
    leverage_ratio = (request.tier1_capital / request.total_assets) if request.total_assets else 0
    liquidity_ratio = (request.liquid_assets / request.net_loan) if request.net_loan else 0

    capital_ratios = [
        RegulatoryRatio(ratio_name="CET1 Ratio", actual_value=round(tier1_ratio, 4), minimum_required=0.045, buffer=round(tier1_ratio - 0.045, 4), compliant=tier1_ratio >= 0.045),
        RegulatoryRatio(ratio_name="Tier 1 Ratio", actual_value=round(tier1_ratio, 4), minimum_required=0.06, buffer=round(tier1_ratio - 0.06, 4), compliant=tier1_ratio >= 0.06),
        RegulatoryRatio(ratio_name="Total Capital Ratio", actual_value=round(total_cap_ratio, 4), minimum_required=0.08, buffer=round(total_cap_ratio - 0.08, 4), compliant=total_cap_ratio >= 0.08)
    ]

    liquidity_ratios = [
        RegulatoryRatio(ratio_name="Liquidity Coverage Ratio", actual_value=round(liquidity_ratio, 2), minimum_required=1.0, buffer=round(liquidity_ratio - 1.0, 2), compliant=liquidity_ratio >= 1.0)
    ]

    leverage_ratios = [
        RegulatoryRatio(ratio_name="Leverage Ratio", actual_value=round(leverage_ratio, 4), minimum_required=0.03, buffer=round(leverage_ratio - 0.03, 4), compliant=leverage_ratio >= 0.03)
    ]

    all_compliant = all(r.compliant for r in capital_ratios + liquidity_ratios + leverage_ratios)
    overall = "compliant" if all_compliant else "non_compliant"

    return RegulatoryRatioResponse(
        company_id=request.company_id,
        regulator_type=request.regulator_type,
        capital_ratios=capital_ratios,
        liquidity_ratios=liquidity_ratios,
        leverage_ratios=leverage_ratios,
        overall_compliance=overall,
        recommendations=["Maintain capital buffers above minimum requirements", "Monitor liquidity ratios regularly", "Review risk-weighted assets calculation"] if not all_compliant else ["Continue monitoring regulatory ratios"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8229)
