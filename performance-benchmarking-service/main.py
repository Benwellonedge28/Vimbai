"""
Vimbai Performance Benchmarking Service
Benchmarks organizational KPIs against industry standards and peer comparisons.
"""

import os
import sys
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "performance-benchmarking-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8280"))

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

# Loaded bare (uvicorn main:app) and by path (root integration tests), so this
# directory has to be importable before the store module is pulled in.
_HERE = os.path.dirname(os.path.abspath(__file__))
if _HERE not in sys.path:
    sys.path.insert(0, _HERE)

import benchmark_store as store  # noqa: E402

app = FastAPI(title="Vimbai Performance Benchmarking Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class BenchmarkMetric(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    category: str  # financial, operational, customer, growth
    unit: str = ""  # percentage, ratio, days, currency
    industry_median: float = 0.0
    industry_top_quartile: float = 0.0
    industry_bottom_quartile: float = 0.0
    description: str = ""


class BenchmarkResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    metric_id: str
    organization_value: float
    industry_median: float
    percentile_rank: float  # 0-100
    rating: str = ""  # below_average, average, above_average, top_quartile
    period: str
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class PeerComparison(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    metric_id: str
    organization_name: str
    value: float
    period: str


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the caller and Book context (set by the gateway) to the store."""
    store.user_id_var.set(request.headers.get("X-User-Id"))
    store.book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


# Metrics, results and peer data are durable records: Neo4j, caller-owned,
# Book-gated.
METRICS = ("BenchmarkMetric", "OWNS_METRIC")
RESULTS = ("BenchmarkResult", "OWNS_RESULT")
PEERS = ("PeerComparison", "OWNS_PEER_COMPARISON")


def _metric_model(node: Dict[str, Any]) -> BenchmarkMetric:
    return BenchmarkMetric(
        id=node["id"],
        name=node.get("name", ""),
        category=node.get("category", ""),
        unit=node.get("unit", ""),
        industry_median=float(node.get("industry_median") or 0),
        industry_top_quartile=float(node.get("industry_top_quartile") or 0),
        industry_bottom_quartile=float(node.get("industry_bottom_quartile") or 0),
        description=node.get("description", ""),
    )


def _result_model(node: Dict[str, Any]) -> BenchmarkResult:
    return BenchmarkResult(
        id=node["id"],
        metric_id=node.get("metric_id", ""),
        organization_value=float(node.get("organization_value") or 0),
        industry_median=float(node.get("industry_median") or 0),
        percentile_rank=float(node.get("percentile_rank") or 0),
        rating=node.get("rating", ""),
        period=node.get("period", ""),
        created_at=node.get("created_at"),
    )


def _peer_model(node: Dict[str, Any]) -> PeerComparison:
    return PeerComparison(
        id=node["id"],
        metric_id=node.get("metric_id", ""),
        organization_name=node.get("organization_name", ""),
        value=float(node.get("value") or 0),
        period=node.get("period", ""),
    )


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/metrics", response_model=BenchmarkMetric)
async def create_metric(
    name: str,
    category: str,
    unit: str = "",
    industry_median: float = 0.0,
    industry_top_quartile: float = 0.0,
    industry_bottom_quartile: float = 0.0,
    description: str = "",
):
    """Define a benchmark metric."""
    valid_cats = ["financial", "operational", "customer", "growth"]
    if category not in valid_cats:
        raise HTTPException(status_code=400, detail=f"Invalid category. Must be one of {valid_cats}")

    metric = BenchmarkMetric(
        name=name,
        category=category,
        unit=unit,
        industry_median=industry_median,
        industry_top_quartile=industry_top_quartile,
        industry_bottom_quartile=industry_bottom_quartile,
        description=description,
    )
    node = await store.create_node(
        METRICS[0],
        METRICS[1],
        {
            "id": metric.id,
            "name": metric.name,
            "category": metric.category,
            "unit": metric.unit,
            "industry_median": metric.industry_median,
            "industry_top_quartile": metric.industry_top_quartile,
            "industry_bottom_quartile": metric.industry_bottom_quartile,
            "description": metric.description,
            "created_at": datetime.now(timezone.utc).isoformat(),
        },
    )
    logger.info("Benchmark metric created", metric_id=metric.id, name=name)
    return _metric_model(node)


@app.get("/metrics", response_model=List[BenchmarkMetric])
async def list_metrics(category: Optional[str] = None):
    """List benchmark metrics."""
    nodes = await store.list_nodes(METRICS[0], METRICS[1], {"category": category})
    return [_metric_model(node) for node in nodes]


@app.post("/benchmark", response_model=BenchmarkResult)
async def benchmark(org_value: float, metric_id: str, period: str):
    """Benchmark an organization's value against industry standards."""
    node = await store.get_node(METRICS[0], METRICS[1], metric_id)
    if not node:
        raise HTTPException(status_code=404, detail="Metric not found")
    metric = _metric_model(node)

    # Calculate percentile rank (simplified)
    if org_value >= metric.industry_top_quartile:
        percentile = 75.0 + ((org_value - metric.industry_top_quartile) / max(metric.industry_top_quartile, 1)) * 25
        rating = "top_quartile"
    elif org_value >= metric.industry_median:
        percentile = (
            50.0
            + ((org_value - metric.industry_median) / max(metric.industry_top_quartile - metric.industry_median, 1))
            * 25
        )
        rating = "above_average"
    elif org_value >= metric.industry_bottom_quartile:
        percentile = (
            25.0
            + (
                (org_value - metric.industry_bottom_quartile)
                / max(metric.industry_median - metric.industry_bottom_quartile, 1)
            )
            * 25
        )
        rating = "average"
    else:
        percentile = max(
            0, ((org_value - metric.industry_bottom_quartile) / max(abs(metric.industry_bottom_quartile), 1)) * 25
        )
        rating = "below_average"

    result = BenchmarkResult(
        metric_id=metric_id,
        organization_value=org_value,
        industry_median=metric.industry_median,
        percentile_rank=round(percentile, 2),
        rating=rating,
        period=period,
    )
    node = await store.create_node(
        RESULTS[0],
        RESULTS[1],
        {
            "id": result.id,
            "metric_id": result.metric_id,
            "organization_value": result.organization_value,
            "industry_median": result.industry_median,
            "percentile_rank": result.percentile_rank,
            "rating": result.rating,
            "period": result.period,
            "created_at": result.created_at.isoformat(),
        },
    )
    logger.info("Benchmark completed", metric_id=metric_id, rating=rating, percentile=percentile)
    return _result_model(node)


@app.get("/results", response_model=List[BenchmarkResult])
async def list_results(metric_id: Optional[str] = None, limit: int = 50):
    """List benchmark results."""
    nodes = await store.list_nodes(RESULTS[0], RESULTS[1], {"metric_id": metric_id})
    return [_result_model(node) for node in nodes][-limit:]


@app.post("/peer-comparisons")
async def add_peer_comparison(metric_id: str, organization_name: str, value: float, period: str):
    """Add peer comparison data for a metric."""
    peer = PeerComparison(
        metric_id=metric_id,
        organization_name=organization_name,
        value=value,
        period=period,
    )
    node = await store.create_node(
        PEERS[0],
        PEERS[1],
        {
            "id": peer.id,
            "metric_id": peer.metric_id,
            "organization_name": peer.organization_name,
            "value": peer.value,
            "period": peer.period,
            "created_at": datetime.now(timezone.utc).isoformat(),
        },
    )
    return _peer_model(node)


@app.get("/peer-comparisons/{metric_id}", response_model=List[PeerComparison])
async def get_peer_comparisons(metric_id: str, period: Optional[str] = None):
    """Get peer comparison data for a metric."""
    nodes = await store.list_nodes(PEERS[0], PEERS[1], {"metric_id": metric_id})
    peers = [_peer_model(node) for node in nodes]
    if period:
        peers = [p for p in peers if p.period == period]
    return peers


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
