"""
Audit Trails Service
Port: 8283
Audit trail analysis and reporting
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Audit Trails Service", version="1.0.0")

class AuditEntry(BaseModel):
    entry_id: str
    user_id: str
    action: str
    timestamp: str
    system: str

class AuditTrailsRequest(BaseModel):
    company_id: str
    entries: List[AuditEntry]
    start_date: str
    end_date: str

class AuditTrailsResponse(BaseModel):
    company_id: str
    audit_summary: Dict[str, Any]
    activity_by_user: List[Dict[str, Any]]
    suspicious_activities: List[Dict[str, Any]]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "audit-trails", "version": "1.0.0"}

@app.post("/analyze", response_model=AuditTrailsResponse)
async def analyze_audit_trails(request: AuditTrailsRequest):
    logger.info("Analyzing audit trails", company=request.company_id)

    by_user = {}
    for e in request.entries:
        if e.user_id not in by_user:
            by_user[e.user_id] = {"user_id": e.user_id, "actions": 0}
        by_user[e.user_id]["actions"] += 1
    
    activity_by_user = list(by_user.values())
    
    suspicious = []
    high_activity = [u for u in activity_by_user if u["actions"] > 100]
    if high_activity:
        suspicious.extend([{"user_id": u["user_id"], "reason": "High activity volume"} for u in high_activity])
    
    audit_summary = {
        "total_entries": len(request.entries),
        "unique_users": len(by_user),
        "start_date": request.start_date,
        "end_date": request.end_date,
        "avg_actions_per_user": round(len(request.entries) / len(by_user), 2) if by_user else 0
    }
    
    return AuditTrailsResponse(
        company_id=request.company_id,
        audit_summary=audit_summary,
        activity_by_user=activity_by_user,
        suspicious_activities=suspicious
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8283)
