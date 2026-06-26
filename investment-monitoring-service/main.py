"""
Investment Monitoring Service
Port: 8290
Investment portfolio monitoring
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Investment Monitoring Service", version="1.0.0")

class Investment(BaseModel):
    investment_id: str
    name: str
    cost: float
    current_value: float
    target_return: float

class InvestmentMonitoringRequest(BaseModel):
    company_id: str
    investments: List[Investment]
    monitoring_date: str

class InvestmentMonitoringResponse(BaseModel):
    company_id: str
    monitoring_date: str
    portfolio_summary: Dict[str, Any]
    underperformers: List[Dict[str, Any]]
    alerts: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "investment-monitoring", "version": "1.0.0"}

@app.post("/monitor", response_model=InvestmentMonitoringResponse)
async def monitor_investments(request: InvestmentMonitoringRequest):
    logger.info("Monitoring investments", company=request.company_id)

    total_cost = sum(i.cost for i in request.investments)
    total_value = sum(i.current_value for i in request.investments)
    
    underperformers = []
    for inv in request.investments:
        return_pct = (inv.current_value - inv.cost) / inv.cost * 100 if inv.cost else 0
        if return_pct < inv.target_return * 0.8:
            underperformers.append({
                "investment_id": inv.investment_id,
                "name": inv.name,
                "return_pct": round(return_pct, 2),
                "target_return": round(inv.target_return * 100, 2)
            })
    
    portfolio_summary = {
        "total_investments": len(request.investments),
        "total_cost": round(total_cost, 2),
        "current_value": round(total_value, 2),
        "total_gain": round(total_value - total_cost, 2),
        "return_pct": round((total_value - total_cost) / total_cost * 100, 2) if total_cost else 0
    }
    
    alerts = [f"{u['name']} underperforming target" for u in underperformers]
    
    return InvestmentMonitoringResponse(
        company_id=request.company_id,
        monitoring_date=request.monitoring_date,
        portfolio_summary=portfolio_summary,
        underperformers=underperformers,
        alerts=alerts
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8290)
