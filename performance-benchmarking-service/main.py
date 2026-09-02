"""
Vimbai Performance Benchmarking Service
Benchmarks organizational KPIs against industry standards and peer comparisons.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
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


metrics: List[BenchmarkMetric] = []
results: List[BenchmarkResult] = []
peer_data: List[PeerComparison] = []


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
    metrics.append(metric)
    logger.info("Benchmark metric created", metric_id=metric.id, name=name)
    return metric


@app.get("/metrics", response_model=List[BenchmarkMetric])
async def list_metrics(category: Optional[str] = None):
    """List benchmark metrics."""
    if category:
        return [m for m in metrics if m.category == category]
    return metrics


@app.post("/benchmark", response_model=BenchmarkResult)
async def benchmark(org_value: float, metric_id: str, period: str):
    """Benchmark an organization's value against industry standards."""
    metric = next((m for m in metrics if m.id == metric_id), None)
    if not metric:
        raise HTTPException(status_code=404, detail="Metric not found")

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
    results.append(result)
    logger.info("Benchmark completed", metric_id=metric_id, rating=rating, percentile=percentile)
    return result


@app.get("/results", response_model=List[BenchmarkResult])
async def list_results(metric_id: Optional[str] = None, limit: int = 50):
    """List benchmark results."""
    result_list = results
    if metric_id:
        result_list = [r for r in result_list if r.metric_id == metric_id]
    return result_list[-limit:]


@app.post("/peer-comparisons")
async def add_peer_comparison(metric_id: str, organization_name: str, value: float, period: str):
    """Add peer comparison data for a metric."""
    peer = PeerComparison(
        metric_id=metric_id,
        organization_name=organization_name,
        value=value,
        period=period,
    )
    peer_data.append(peer)
    return peer


@app.get("/peer-comparisons/{metric_id}", response_model=List[PeerComparison])
async def get_peer_comparisons(metric_id: str, period: Optional[str] = None):
    """Get peer comparison data for a metric."""
    result = [p for p in peer_data if p.metric_id == metric_id]
    if period:
        result = [p for p in result if p.period == period]
    return result


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
