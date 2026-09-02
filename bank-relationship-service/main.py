"""Vimbai Bank Relationship Service - Manage banking relationships and service quality. Port: 8323"""

import os
import uuid
from collections import defaultdict
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "bank-relationship-service"
PORT = int(os.getenv("PORT", "8323"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Bank Relationship Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="bank-relationship-service", instrument_app=app)
except ImportError:
    TRACER = None


class RelationshipStatus(str, Enum):
    ACTIVE = "active"
    DORMANT = "dormant"
    TERMINATED = "terminated"
    PROSPECTIVE = "prospective"


class BankRelationship(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    bank_name: str
    branch: str = ""
    account_number: str = ""
    relationship_manager: str = ""
    contact_email: str = ""
    contact_phone: str = ""
    services: List[str] = []  # e.g., ["checking", "credit_line", "fx", "trade_finance"]
    status: RelationshipStatus = RelationshipStatus.ACTIVE
    opened_date: Optional[datetime] = None
    rating: int = 3  # 1-5
    notes: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ServiceQualityMetric(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    relationship_id: str
    metric_name: str  # e.g., "response_time", "fee_competitiveness", "online_banking_quality"
    score: int = 1  # 1-5
    notes: str = ""
    recorded_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


_relationships: Dict[str, List[BankRelationship]] = defaultdict(list)
_metrics: Dict[str, List[ServiceQualityMetric]] = defaultdict(list)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/relationships", response_model=BankRelationship)
async def create_relationship(rel: BankRelationship):
    _relationships[rel.company_id].append(rel)
    logger.info("relationship_created", company_id=rel.company_id, bank=rel.bank_name)
    return rel


@app.get("/relationships/{company_id}")
async def get_relationships(company_id: str, status_filter: Optional[str] = None):
    rels = _relationships.get(company_id, [])
    if status_filter:
        rels = [r for r in rels if r.status.value == status_filter]
    return {"company_id": company_id, "relationships": rels, "total": len(rels)}


@app.put("/relationships/{rel_id}")
async def update_relationship(
    rel_id: str, rating: Optional[int] = None, status: Optional[RelationshipStatus] = None, notes: Optional[str] = None
):
    for rels in _relationships.values():
        for r in rels:
            if r.id == rel_id:
                if rating is not None:
                    r.rating = rating
                if status is not None:
                    r.status = status
                if notes is not None:
                    r.notes = notes
                return {"id": rel_id, "rating": r.rating, "status": r.status.value}
    raise HTTPException(status_code=404, detail="Relationship not found")


@app.post("/quality-metrics")
async def add_quality_metric(metric: ServiceQualityMetric):
    _metrics[metric.relationship_id].append(metric)
    return {"id": metric.id, "metric": metric.metric_name, "score": metric.score}


@app.get("/quality-metrics/{relationship_id}")
async def get_quality_metrics(relationship_id: str):
    metrics = _metrics.get(relationship_id, [])
    if not metrics:
        return {"relationship_id": relationship_id, "avg_score": 0, "metrics": []}
    avg = sum(m.score for m in metrics) / len(metrics)
    return {"relationship_id": relationship_id, "avg_score": avg, "metrics": metrics}


@app.get("/summary/{company_id}")
async def relationship_summary(company_id: str):
    rels = _relationships.get(company_id, [])
    active = sum(1 for r in rels if r.status == RelationshipStatus.ACTIVE)
    banks = len(set(r.bank_name for r in rels))
    services = set()
    for r in rels:
        services.update(r.services)
    avg_rating = sum(r.rating for r in rels) / max(1, len(rels))
    return {
        "company_id": company_id,
        "total_relationships": len(rels),
        "active": active,
        "unique_banks": banks,
        "total_services": len(services),
        "services": list(services),
        "avg_rating": avg_rating,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
