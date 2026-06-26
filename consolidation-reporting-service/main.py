"""
Consolidation Reporting Service
Port: 8275
Financial consolidation and elimination
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Consolidation Reporting Service", version="1.0.0")

class Subsidiary(BaseModel):
    entity_id: str
    entity_name: str
    ownership_pct: float
    revenue: float
    assets: float
    intercompany_balance: float

class ConsolidationReportingRequest(BaseModel):
    company_id: str
    parent_id: str
    subsidiaries: List[Subsidiary]
    intercompany_eliminations: Dict[str, float]

class ConsolidationReportingResponse(BaseModel):
    company_id: str
    report_date: str
    consolidated_metrics: Dict[str, Any]
    entity_breakdown: List[Dict[str, Any]]
    elimination_summary: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "consolidation-reporting", "version": "1.0.0"}

@app.post("/consolidate", response_model=ConsolidationReportingResponse)
async def consolidate_entities(request: ConsolidationReportingRequest):
    logger.info("Consolidating entities", company=request.company_id)

    entity_breakdown = []
    total_revenue = 0
    total_assets = 0
    total_interco = 0
    
    for sub in request.subsidiaries:
        revenue_contribution = sub.revenue * (sub.ownership_pct / 100)
        asset_contribution = sub.assets * (sub.ownership_pct / 100)
        
        entity_breakdown.append({
            "entity_id": sub.entity_id,
            "entity_name": sub.entity_name,
            "ownership": round(sub.ownership_pct, 2),
            "revenue": round(sub.revenue, 2),
            "revenue_contribution": round(revenue_contribution, 2),
            "assets": round(sub.assets, 2),
            "asset_contribution": round(asset_contribution, 2),
            "intercompany_balance": round(sub.intercompany_balance, 2)
        })
        
        total_revenue += sub.revenue
        total_assets += sub.assets
        total_interco += sub.intercompany_balance
    
    consolidated_metrics = {
        "total_entities": len(request.subsidiaries),
        "total_revenue": round(total_revenue, 2),
        "total_assets": round(total_assets, 2),
        "total_intercompany": round(total_interco, 2),
        "minority_interest": round(total_assets * 0.05, 2)
    }
    
    elimination_summary = {
        "intercompany_revenue": round(total_interco * 0.5, 2),
        "intercompany_expense": round(total_interco * 0.5, 2),
        "intercompany_balance": round(total_interco, 2),
        "net_elimination": 0
    }
    
    recommendations = []
    if len(request.subsidiaries) > 20:
        recommendations.append("Large number of entities - ensure proper governance")
    if total_interco > total_revenue * 0.2:
        recommendations.append("High intercompany transactions - verify arm's length pricing")

    return ConsolidationReportingResponse(
        company_id=request.company_id,
        report_date=datetime.now().isoformat(),
        consolidated_metrics=consolidated_metrics,
        entity_breakdown=entity_breakdown,
        elimination_summary=elimination_summary,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8275)
