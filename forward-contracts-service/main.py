"""
Forward Contracts Service
Port: 8247
Forward contract valuation and analysis
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
import math

logger = structlog.get_logger()
app = FastAPI(title="Forward Contracts Service", version="1.0.0")

class ForwardContract(BaseModel):
    contract_id: str
    notional: float
    spot_rate: float
    forward_rate: float
    delivery_date: str
    currency_pair: str

class ForwardContractRequest(BaseModel):
    company_id: str
    contracts: List[ForwardContract]
    current_spot_rate: float
    risk_free_rate_domestic: float
    risk_free_rate_foreign: float
    volatility: float

class ForwardContractResponse(BaseModel):
    company_id: str
    portfolio_valuation: Dict[str, Any]
    contract_analysis: List[Dict[str, Any]]
    exposure_summary: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "forward-contracts", "version": "1.0.0"}

@app.post("/analyze", response_model=ForwardContractResponse)
async def analyze_forward_contracts(request: ForwardContractRequest):
    logger.info("Analyzing forward contracts", company=request.company_id)

    contract_analysis = []
    total_mtm = 0
    total_exposure = 0
    
    for c in request.contracts:
        time_to_delivery = 0.5
        
        theoretical_forward = request.current_spot_rate * math.exp((request.risk_free_rate_domestic - request.risk_free_rate_foreign) * time_to_delivery)
        forward_points = c.forward_rate - c.spot_rate
        
        mtm_value = c.notional * (c.forward_rate - request.current_spot_rate)
        mtm_pct = (mtm_value / c.notional) * 100
        
        sensitivity = c.notional * time_to_delivery * request.volatility
        
        contract_analysis.append({
            "contract_id": c.contract_id,
            "currency_pair": c.currency_pair,
            "notional": c.notional,
            "spot_rate": c.spot_rate,
            "forward_rate": c.forward_rate,
            "market_value": round(mtm_value, 2),
            "market_value_pct": round(mtm_pct, 4),
            "theoretical_forward": round(theoretical_forward, 4),
            "forward_points": round(forward_points, 4),
            "delta": round(mtm_value, 2),
            "gamma_risk": round(sensitivity, 2)
        })
        
        total_mtm += mtm_value
        total_exposure += c.notional
    
    exposure_summary = {
        "long_contracts": sum(1 for c in contract_analysis if c["market_value"] > 0),
        "short_contracts": sum(1 for c in contract_analysis if c["market_value"] < 0),
        "total_exposure": total_exposure,
        "net_mtm": round(total_mtm, 2),
        "hedge_ratio": abs(total_mtm) / total_exposure if total_exposure else 0
    }
    
    recommendations = []
    if exposure_summary["net_mtm"] < -total_exposure * 0.1:
        recommendations.append("Significant unrealized loss - consider early termination or renegotiation")
    if exposure_summary["hedge_ratio"] < 0.5:
        recommendations.append("Under-hedged position - consider adding forward contracts")
    if exposure_summary["hedge_ratio"] > 1.2:
        recommendations.append("Over-hedged - reduce exposure to avoid speculative position")

    return ForwardContractResponse(
        company_id=request.company_id,
        portfolio_valuation={"total_mtm": round(total_mtm, 2), "total_exposure": total_exposure},
        contract_analysis=contract_analysis,
        exposure_summary=exposure_summary,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8247)
