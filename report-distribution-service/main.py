"""
Report Distribution Service
Port: 8273
Automated report distribution management
"""
from datetime import datetime
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Report Distribution Service", version="1.0.0")

class DistributionRule(BaseModel):
    rule_id: str
    report_type: str
    recipients: List[str]
    delivery_method: str
    frequency: str

class ReportDistributionRequest(BaseModel):
    company_id: str
    rules: List[DistributionRule]
    recent_distributions: List[Dict[str, Any]]

class ReportDistributionResponse(BaseModel):
    company_id: str
    distribution_summary: Dict[str, Any]
    recent_activity: List[Dict[str, Any]]
    delivery_stats: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "report-distribution", "version": "1.0.0"}

@app.post("/analyze", response_model=ReportDistributionResponse)
async def analyze_distribution(request: ReportDistributionRequest):
    logger.info("Analyzing report distribution", company=request.company_id)

    recent_activity = []
    for d in request.recent_distributions:
        recent_activity.append({
            "report": d.get("report_name", "Unknown"),
            "sent_date": d.get("date", "N/A"),
            "recipients": d.get("recipient_count", 0),
            "status": d.get("status", "Sent")
        })
    
    total_sent = sum(d.get("recipient_count", 0) for d in request.recent_distributions)
    successful = sum(1 for d in request.recent_distributions if d.get("status") == "Sent")
    
    distribution_summary = {
        "total_rules": len(request.rules),
        "total_recipients": sum(len(r.recipients) for r in request.rules),
        "reports_sent": len(request.recent_distributions),
        "total_deliveries": total_sent
    }
    
    delivery_stats = {
        "success_rate": round(successful / len(request.recent_distributions) * 100, 2) if request.recent_distributions else 0,
        "avg_recipients_per_report": round(total_sent / len(request.recent_distributions), 2) if request.recent_distributions else 0,
        "email_count": sum(1 for r in request.rules if r.delivery_method == "email"),
        "portal_count": sum(1 for r in request.rules if r.delivery_method == "portal")
    }
    
    recommendations = []
    if delivery_stats["success_rate"] < 95:
        recommendations.append("Delivery success rate below 95% - investigate failures")

    return ReportDistributionResponse(
        company_id=request.company_id,
        distribution_summary=distribution_summary,
        recent_activity=recent_activity,
        delivery_stats=delivery_stats,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8273)
