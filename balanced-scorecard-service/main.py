"""
Balanced Scorecard Service
Port: 8279
Balanced scorecard implementation
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Balanced Scorecard Service", version="1.0.0")

class ScorecardMetric(BaseModel):
    perspective: str
    metric_name: str
    value: float
    target: float
    weight: float

class BalancedScorecardRequest(BaseModel):
    company_id: str
    metrics: List[ScorecardMetric]

class BalancedScorecardResponse(BaseModel):
    company_id: str
    assessment_date: str
    perspective_scores: Dict[str, Any]
    overall_score: float
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "balanced-scorecard", "version": "1.0.0"}

@app.post("/assess", response_model=BalancedScorecardResponse)
async def assess_scorecard(request: BalancedScorecardRequest):
    logger.info("Assessing balanced scorecard", company=request.company_id)

    perspectives = ["Financial", "Customer", "Internal Process", "Learning & Growth"]
    perspective_scores = {}
    
    for persp in perspectives:
        persp_metrics = [m for m in request.metrics if m.perspective == persp]
        if persp_metrics:
            total_weight = sum(m.weight for m in persp_metrics)
            weighted_score = sum((m.value / m.target * 100) * m.weight for m in persp_metrics) / total_weight if total_weight else 0
            perspective_scores[persp] = {
                "score": round(weighted_score, 2),
                "metrics_count": len(persp_metrics)
            }
    
    overall_score = sum(ps["score"] for ps in perspective_scores.values()) / len(perspective_scores) if perspective_scores else 0
    
    recommendations = []
    if overall_score < 80:
        recommendations.append("Overall scorecard below target - focus on weakest perspective")
    
    return BalancedScorecardResponse(
        company_id=request.company_id,
        assessment_date=datetime.now().isoformat(),
        perspective_scores=perspective_scores,
        overall_score=round(overall_score, 2),
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8279)
