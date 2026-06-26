"""
Segment Reporting Service
Port: 8270
Business segment reporting
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Segment Reporting Service", version="1.0.0")

class Segment(BaseModel):
    segment_id: str
    segment_name: str
    revenue: float
    ebitda: float
    assets: float
    capex: float

class SegmentReportingRequest(BaseModel):
    company_id: str
    segments: List[Segment]
    total_revenue: float
    total_assets: float

class SegmentReportingResponse(BaseModel):
    company_id: str
    report_date: str
    segment_analysis: List[Dict[str, Any]]
    segment_metrics: Dict[str, Any]
    concentration_analysis: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "segment-reporting", "version": "1.0.0"}

@app.post("/analyze", response_model=SegmentReportingResponse)
async def analyze_segments(request: SegmentReportingRequest):
    logger.info("Analyzing segments", company=request.company_id)

    segment_analysis = []
    for seg in request.segments:
        margin = seg.ebitda / seg.revenue * 100 if seg.revenue else 0
        roa = seg.ebitda / seg.assets * 100 if seg.assets else 0
        
        segment_analysis.append({
            "segment_id": seg.segment_id,
            "segment_name": seg.segment_name,
            "revenue": round(seg.revenue, 2),
            "ebitda": round(seg.ebitda, 2),
            "margin": round(margin, 2),
            "assets": round(seg.assets, 2),
            "roa": round(roa, 2),
            "capex": round(seg.capex, 2),
            "revenue_share": round(seg.revenue / request.total_revenue * 100, 2) if request.total_revenue else 0,
            "asset_share": round(seg.assets / request.total_assets * 100, 2) if request.total_assets else 0
        })
    
    segment_analysis.sort(key=lambda x: x["revenue"], reverse=True)
    
    top_segment = segment_analysis[0] if segment_analysis else {}
    concentration = top_segment.get("revenue_share", 0)
    
    segment_metrics = {
        "total_segments": len(request.segments),
        "total_revenue": round(request.total_revenue, 2),
        "total_ebitda": round(sum(s.ebitda for s in request.segments), 2),
        "total_assets": round(request.total_assets, 2),
        "avg_margin": round(sum(s.ebitda for s in request.segments) / request.total_revenue * 100, 2) if request.total_revenue else 0
    }
    
    concentration_analysis = {
        "top_segment_concentration": round(concentration, 2),
        "hhi": round(sum((s.revenue / request.total_revenue * 100) ** 2 for s in request.segments), 2) if request.total_revenue else 0,
        "diversification": "Concentrated" if concentration > 60 else "Moderate" if concentration > 40 else "Diversified"
    }
    
    recommendations = []
    if concentration > 70:
        recommendations.append("High segment concentration - consider diversification")
    low_margin = [s for s in segment_analysis if s["margin"] < 10]
    if low_margin:
        recommendations.append(f"{len(low_margin)} segments with low margins - review profitability")

    return SegmentReportingResponse(
        company_id=request.company_id,
        report_date=datetime.now().isoformat(),
        segment_analysis=segment_analysis,
        segment_metrics=segment_metrics,
        concentration_analysis=concentration_analysis,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8270)
