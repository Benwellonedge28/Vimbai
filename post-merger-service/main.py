"""
Post-Merger Integration Service
Port: 8244
M&A post-merger integration tracking
"""

from datetime import datetime
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Post-Merger Integration Service", version="1.0.0")


class IntegrationMilestone(BaseModel):
    milestone_id: str
    description: str
    planned_date: str
    actual_date: str
    status: str
    completion_percentage: float


class PostMergerRequest(BaseModel):
    deal_id: str
    acquirer_id: str
    target_id: str
    synergies_achieved: float
    synergies_target: float
    integration_costs_actual: float
    integration_costs_budget: float
    days_since_close: int
    milestones: List[IntegrationMilestone]


class PostMergerResponse(BaseModel):
    deal_id: str
    status_date: str
    synergy_performance: Dict[str, Any]
    cost_performance: Dict[str, Any]
    integration_progress: Dict[str, Any]
    risk_areas: List[str]
    overall_status: str
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "post-merger", "version": "1.0.0"}


@app.post("/track", response_model=PostMergerResponse)
async def track_integration(request: PostMergerRequest):
    logger.info("Tracking post-merger integration", deal=request.deal_id)

    synergy_rate = request.synergies_achieved / request.synergies_target if request.synergies_target else 0
    synergy_variance = request.synergies_achieved - request.synergies_target
    synergy_status = "On Track" if synergy_rate >= 0.9 else "Behind" if synergy_rate >= 0.7 else "Critical"

    synergy_performance = {
        "target": request.synergies_target,
        "achieved": request.synergies_achieved,
        "rate": round(synergy_rate * 100, 2),
        "variance": round(synergy_variance, 2),
        "status": synergy_status,
        "daily_rate": (
            round(request.synergies_achieved / request.days_since_close, 2) if request.days_since_close else 0
        ),
    }

    cost_variance = request.integration_costs_actual - request.integration_costs_budget
    cost_rate = (
        request.integration_costs_actual / request.integration_costs_budget if request.integration_costs_budget else 0
    )

    cost_performance = {
        "budget": request.integration_costs_budget,
        "actual": request.integration_costs_actual,
        "variance": round(cost_variance, 2),
        "variance_percentage": round(
            (cost_variance / request.integration_costs_budget * 100) if request.integration_costs_budget else 0, 2
        ),
        "status": "Over Budget" if cost_variance > 0 else "Under Budget",
    }

    completed = sum(1 for m in request.milestones if m.status == "Completed")
    in_progress = sum(1 for m in request.milestones if m.status == "In Progress")
    delayed = sum(1 for m in request.milestones if m.status == "Delayed")
    avg_completion = (
        sum(m.completion_percentage for m in request.milestones) / len(request.milestones) if request.milestones else 0
    )

    integration_progress = {
        "total_milestones": len(request.milestones),
        "completed": completed,
        "in_progress": in_progress,
        "delayed": delayed,
        "average_completion": round(avg_completion, 2),
        "days_since_close": request.days_since_close,
    }

    risk_areas = []
    if synergy_rate < 0.8:
        risk_areas.append("Synergy realization behind schedule")
    if cost_variance > request.integration_costs_budget * 0.1:
        risk_areas.append("Integration costs exceeding budget")
    if delayed > len(request.milestones) * 0.3:
        risk_areas.append("Multiple milestone delays")

    overall_status = "Green" if not risk_areas else "Amber" if len(risk_areas) <= 1 else "Red"

    recommendations = []
    if synergy_rate < 0.9:
        recommendations.append("Accelerate synergy capture - consider additional resources")
    if delayed > 2:
        recommendations.append("Review delayed milestones and reallocate resources")
    if cost_variance > 0:
        recommendations.append("Cost overrun requires immediate attention - review scope")

    return PostMergerResponse(
        deal_id=request.deal_id,
        status_date=datetime.now().isoformat(),
        synergy_performance=synergy_performance,
        cost_performance=cost_performance,
        integration_progress=integration_progress,
        risk_areas=risk_areas,
        overall_status=overall_status,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8244)
