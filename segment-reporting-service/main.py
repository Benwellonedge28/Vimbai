"""
Segment Reporting Service
Port: 8374
Operating segment reporting (ASC 280/IFRS 8)
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from datetime import datetime
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Segment Reporting Service", version="1.0.0")

class SegmentRequest(BaseModel):
    company_id: str
    segments: List[Dict[str, Any]]
    reconciliation_items: List[Dict[str, Any]]

class SegmentResponse(BaseModel):
    company_id: str
    total_revenue: float
    segment_results: List[Dict[str, Any]]
    reconciliation: Dict[str, float]
    consolidated_result: Dict[str, float]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "segment-reporting", "version": "1.0.0"}

@app.post("/report", response_model=SegmentResponse)
async def prepare_segment_report(request: SegmentRequest):
    logger.info("Preparing segment report", company=request.company_id)
    
    total_rev = 0.0
    segment_results = []
    
    for seg in request.segments:
        rev = seg.get("revenue", 0)
        profit = seg.get("profit", rev * 0.15)
        total_rev += rev
        segment_results.append({
            "segment_id": seg.get("segment_id"),
            "revenue": rev,
            "operating_profit": round(profit, 2),
            "assets": seg.get("assets", rev * 2)
        })
    
    return SegmentResponse(
        company_id=request.company_id,
        total_revenue=round(total_rev, 2),
        segment_results=segment_results,
        reconciliation={"eliminations": -50000.0},
        consolidated_result={"revenue": round(total_rev - 50000, 2), "profit": round(total_rev * 0.15 - 5000, 2)}
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8374)
