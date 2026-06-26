"""
KPI Tracking Service
Port: 8278
Key Performance Indicator tracking
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="KPI Tracking Service", version="1.0.0")

class KPI(BaseModel):
    kpi_id: str
    kpi_name: str
    current_value: float
    target: float
    previous_value: float
    unit: str

class KPITrackingRequest(BaseModel):
    company_id: str
    kpis: List[KPI]
    reporting_period: str

class KPITrackingResponse(BaseModel):
    company_id: str
    period: str
    kpi_summary: Dict[str, Any]
    kpi_analysis: List[Dict[str, Any]]
    trend_analysis: List[Dict[str, Any]]
    alerts: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "kpi-tracking", "version": "1.0.0"}

@app.post("/track", response_model=KPITrackingResponse)
async def track_kpis(request: KPITrackingRequest):
    logger.info("Tracking KPIs", company=request.company_id)

    kpi_analysis = []
    trend_analysis = []
    alerts = []
    
    for kpi in request.kpis:
        achievement = (kpi.current_value / kpi.target * 100) if kpi.target else 0
        change = kpi.current_value - kpi.previous_value
        change_pct = (change / kpi.previous_value * 100) if kpi.previous_value else 0
        
        if achievement < 90:
            alerts.append(f"{kpi.kpi_name} below target by {100-achievement:.1f}%")
        
        kpi_analysis.append({
            "kpi_id": kpi.kpi_id,
            "kpi_name": kpi.kpi_name,
            "current": round(kpi.current_value, 2),
            "target": round(kpi.target, 2),
            "achievement": round(achievement, 2),
            "unit": kpi.unit,
            "status": "On Track" if achievement >= 100 else "At Risk" if achievement >= 90 else "Below Target"
        })
        
        trend_analysis.append({
            "kpi_name": kpi.kpi_name,
            "current": round(kpi.current_value, 2),
            "previous": round(kpi.previous_value, 2),
            "change": round(change, 2),
            "change_pct": round(change_pct, 2),
            "trend": "Improving" if change > 0 else "Declining"
        })
    
    kpi_summary = {
        "total_kpis": len(request.kpis),
        "on_track": sum(1 for k in kpi_analysis if k["status"] == "On Track"),
        "at_risk": sum(1 for k in kpi_analysis if k["status"] == "At Risk"),
        "below_target": sum(1 for k in kpi_analysis if k["status"] == "Below Target"),
        "overall_health": round(sum(k["achievement"] for k in kpi_analysis) / len(kpi_analysis), 2) if kpi_analysis else 0
    }
    
    return KPITrackingResponse(
        company_id=request.company_id,
        period=request.reporting_period,
        kpi_summary=kpi_summary,
        kpi_analysis=kpi_analysis,
        trend_analysis=trend_analysis,
        alerts=alerts
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8278)
