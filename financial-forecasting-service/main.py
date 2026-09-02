"""
Vimbai Financial Forecasting Service
Time-series financial forecasting with multiple methods (linear, exponential, moving average).
Port: 8392
"""

import math
import os
import uuid
from typing import Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "financial-forecasting-service"
PORT = int(os.getenv("PORT", "8392"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Financial Forecasting Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class ForecastRequest(BaseModel):
    company_id: str
    metric_name: str
    historical_values: List[float]  # e.g., quarterly revenue
    periods_ahead: int = 4
    method: str = "auto"  # linear, exponential, moving_average, auto


class ForecastResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    metric_name: str
    forecast: List[float]
    method_used: str
    confidence_interval_lower: List[float]
    confidence_interval_upper: List[float]
    trend: str
    growth_rate: float
    r_squared: float = 0


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


def _linear_regression(values):
    n = len(values)
    if n < 2:
        return 0, values[-1] if values else 0, 0
    x_sum = n * (n - 1) / 2
    y_sum = sum(values)
    xy_sum = sum(i * v for i, v in enumerate(values))
    x2_sum = sum(i * i for i in range(n))
    denom = n * x2_sum - x_sum * x_sum
    if denom == 0:
        return 0, y_sum / n, 0
    slope = (n * xy_sum - x_sum * y_sum) / denom
    intercept = (y_sum - slope * x_sum) / n
    y_pred = [slope * i + intercept for i in range(n)]
    ss_res = sum((v - p) ** 2 for v, p in zip(values, y_pred))
    ss_tot = sum((v - y_sum / n) ** 2 for v in values)
    r2 = 1 - ss_res / ss_tot if ss_tot else 0
    return slope, intercept, r2


def _exponential_smoothing(values, alpha=0.3):
    if not values:
        return []
    result = [values[0]]
    for i in range(1, len(values)):
        result.append(alpha * values[i] + (1 - alpha) * result[-1])
    return result


@app.post("/forecast", response_model=ForecastResult)
async def forecast(req: ForecastRequest):
    values = req.historical_values
    n = len(values)

    if n < 3:
        return ForecastResult(
            company_id=req.company_id,
            metric_name=req.metric_name,
            forecast=[values[-1]] * req.periods_ahead if values else [0] * req.periods_ahead,
            method_used="naive",
            confidence_interval_lower=[0] * req.periods_ahead,
            confidence_interval_upper=[0] * req.periods_ahead,
            trend="insufficient_data",
            growth_rate=0,
            r_squared=0,
        )

    method = req.method
    if method == "auto":
        slope, intercept, r2 = _linear_regression(values)
        method = "linear" if r2 > 0.7 else "exponential"

    if method == "linear":
        slope, intercept, r2 = _linear_regression(values)
        forecast = [slope * (n + i) + intercept for i in range(req.periods_ahead)]
        growth_rate = (slope / intercept * 100) if intercept else 0
        trend = "increasing" if slope > 0 else "decreasing" if slope < 0 else "stable"
        std_err = math.sqrt(sum((v - (slope * i + intercept)) ** 2 for i, v in enumerate(values)) / max(n - 2, 1))
        lower = [f - 1.96 * std_err for f in forecast]
        upper = [f + 1.96 * std_err for f in forecast]
    else:
        smoothed = _exponential_smoothing(values)
        last = smoothed[-1]
        forecast = [
            last * (1 + (smoothed[-1] - smoothed[0]) / max(abs(smoothed[0]), 1) * (i + 1) / n)
            for i in range(req.periods_ahead)
        ]
        r2 = 0
        growth_rate = (smoothed[-1] - smoothed[0]) / smoothed[0] * 100 if smoothed[0] else 0
        trend = "increasing" if smoothed[-1] > smoothed[0] else "decreasing" if smoothed[-1] < smoothed[0] else "stable"
        std_err = math.sqrt(sum((v - s) ** 2 for v, s in zip(values, smoothed)) / max(n - 1, 1))
        lower = [f - 1.96 * std_err for f in forecast]
        upper = [f + 1.96 * std_err for f in forecast]

    return ForecastResult(
        company_id=req.company_id,
        metric_name=req.metric_name,
        forecast=[round(f, 2) for f in forecast],
        method_used=method,
        confidence_interval_lower=[round(l, 2) for l in lower],
        confidence_interval_upper=[round(u, 2) for u in upper],
        trend=trend,
        growth_rate=round(growth_rate, 2),
        r_squared=round(r2, 4),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
