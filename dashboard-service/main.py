"""
Dashboard Service
Port: 8365
Financial dashboard data aggregation
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from datetime import datetime
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Dashboard Service", version="1.0.0")

class DashboardRequest(BaseModel):
    company_id: str
    dashboard_type: str
    refresh_interval: int

class DashboardResponse(BaseModel):
    dashboard_id: str
    company_id: str
    widgets: List[Dict[str, Any]]
    last_refreshed: datetime

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "dashboard", "version": "1.0.0"}

@app.post("/load", response_model=DashboardResponse)
async def load_dashboard(request: DashboardRequest):
    logger.info("Loading dashboard", company=request.company_id, type=request.dashboard_type)
    
    return DashboardResponse(
        dashboard_id=f"DASH-{datetime.now().strftime('%Y%m%d%H%M')}",
        company_id=request.company_id,
        widgets=[
            {"type": "kpi_card", "title": "Revenue", "value": 1500000},
            {"type": "chart", "title": "Trends", "chart_type": "line"},
            {"type": "table", "title": "Top Accounts"}
        ],
        last_refreshed=datetime.now()
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8365)
