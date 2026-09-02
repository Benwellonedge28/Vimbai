"""
Vimbai Balanced Scorecard Service
Kaplan-Norton balanced scorecard with four perspectives and strategic alignment.
Port: 8390
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "balanced-scorecard-service"
PORT = int(os.getenv("PORT", "8390"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Balanced Scorecard Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class KPI(BaseModel):
    name: str
    perspective: str  # financial, customer, internal, learning_growth
    target: float
    actual: float
    weight: float = 1.0
    unit: str = "number"


class ScorecardRequest(BaseModel):
    company_id: str
    period: str
    kpis: List[KPI]


class ScorecardResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period: str
    financial_score: float
    customer_score: float
    internal_score: float
    learning_score: float
    overall_score: float
    perspectives: Dict[str, Dict] = {}
    action_items: List[str] = []


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/score", response_model=ScorecardResult)
async def calculate_scorecard(req: ScorecardRequest):
    perspectives = {
        "financial": {"kpis": [], "score": 0, "weight_total": 0},
        "customer": {"kpis": [], "score": 0, "weight_total": 0},
        "internal": {"kpis": [], "score": 0, "weight_total": 0},
        "learning_growth": {"kpis": [], "score": 0, "weight_total": 0},
    }

    action_items = []

    for kpi in req.kpis:
        p = perspectives.get(kpi.perspective, perspectives["financial"])
        achievement = min(kpi.actual / kpi.target, 1.0) * 100 if kpi.target > 0 else 100
        weighted_score = achievement * kpi.weight
        p["kpis"].append(
            {
                "name": kpi.name,
                "target": kpi.target,
                "actual": kpi.actual,
                "achievement_pct": round(achievement, 1),
                "weight": kpi.weight,
            }
        )
        p["score"] += weighted_score
        p["weight_total"] += kpi.weight

        if achievement < 80:
            action_items.append(f"{kpi.name}: Below target ({achievement:.0f}%) - review strategy")

    for key in perspectives:
        wt = perspectives[key]["weight_total"]
        perspectives[key]["score"] = round(perspectives[key]["score"] / wt * 100, 1) if wt else 0

    fin = perspectives["financial"]["score"]
    cust = perspectives["customer"]["score"]
    internal = perspectives["internal"]["score"]
    learning = perspectives["learning_growth"]["score"]
    overall = round((fin + cust + internal + learning) / 4, 1)

    if overall < 70:
        action_items.insert(0, "Overall performance below 70% - strategic realignment needed")
    elif overall >= 90:
        action_items.insert(0, "Excellent performance across all perspectives")

    return ScorecardResult(
        company_id=req.company_id,
        period=req.period,
        financial_score=fin,
        customer_score=cust,
        internal_score=internal,
        learning_score=learning,
        overall_score=overall,
        perspectives={k: {kk: vv for kk, vv in v.items() if kk != "weight_total"} for k, v in perspectives.items()},
        action_items=action_items or ["All KPIs on track"],
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
