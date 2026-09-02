"""
Management Dashboard Service
Port: 8271
Real-time management dashboard data
"""

from datetime import datetime
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Management Dashboard Service", version="1.0.0")


class DashboardMetric(BaseModel):
    metric_name: str
    current_value: float
    target_value: float
    trend: str
    kpi_category: str


class ManagementDashboardRequest(BaseModel):
    company_id: str
    metrics: List[DashboardMetric]
    period_end: str


class ManagementDashboardResponse(BaseModel):
    company_id: str
    dashboard_date: str
    kpi_summary: Dict[str, Any]
    metric_analysis: List[Dict[str, Any]]
    alerts: List[str]
    top_performers: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "management-dashboard", "version": "1.0.0"}


@app.post("/generate", response_model=ManagementDashboardResponse)
async def generate_dashboard(request: ManagementDashboardRequest):
    logger.info("Generating management dashboard", company=request.company_id)

    metric_analysis = []
    on_target = 0
    above_target = 0
    below_target = 0
    alerts = []

    for m in request.metrics:
        pct_of_target = m.current_value / m.target_value * 100 if m.target_value else 0
        variance = m.current_value - m.target_value

        if pct_of_target >= 100:
            status = "Above Target"
            above_target += 1
        elif pct_of_target >= 90:
            status = "On Target"
            on_target += 1
        else:
            status = "Below Target"
            below_target += 1
            alerts.append(f"{m.metric_name} is {abs(100-pct_of_target):.1f}% below target")

        metric_analysis.append(
            {
                "metric": m.metric_name,
                "current": round(m.current_value, 2),
                "target": round(m.target_value, 2),
                "variance": round(variance, 2),
                "pct_of_target": round(pct_of_target, 2),
                "trend": m.trend,
                "status": status,
            }
        )

    metric_analysis.sort(key=lambda x: x["pct_of_target"], reverse=True)

    kpi_summary = {
        "total_metrics": len(request.metrics),
        "on_target": on_target,
        "above_target": above_target,
        "below_target": below_target,
        "health_score": round((on_target + above_target) / len(request.metrics) * 100, 2) if request.metrics else 0,
    }

    top_performers = [m["metric"] for m in metric_analysis[:3]]

    return ManagementDashboardResponse(
        company_id=request.company_id,
        dashboard_date=datetime.now().isoformat(),
        kpi_summary=kpi_summary,
        metric_analysis=metric_analysis,
        alerts=alerts,
        top_performers=top_performers,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8271)
