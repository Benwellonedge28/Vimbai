"""
Vimbai Rolling Forecast Service
Creates rolling forecasts that update periodically with actual data.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "rolling-forecast-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8173"))

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

app = FastAPI(title="Vimbai Rolling Forecast Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class ForecastPeriod(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    period: str  # YYYY-MM
    forecast_value: float
    actual_value: Optional[float] = None
    variance: Optional[float] = None
    variance_pct: Optional[float] = None
    is_actual: bool = False


class RollingForecast(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    metric: str  # revenue, expenses, cash_flow, headcount
    frequency: str = "monthly"  # weekly, monthly, quarterly
    horizon_periods: int = 12
    periods: List[ForecastPeriod] = []
    current_period: str = ""
    last_updated: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


forecasts: List[RollingForecast] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/forecasts", response_model=RollingForecast)
async def create_forecast(
    name: str,
    metric: str,
    frequency: str = "monthly",
    horizon_periods: int = 12,
    forecast_values: List[Dict[str, Any]] = [],
):
    """Create a rolling forecast."""
    valid_freqs = ["weekly", "monthly", "quarterly"]
    if frequency not in valid_freqs:
        raise HTTPException(status_code=400, detail=f"Invalid frequency. Must be one of {valid_freqs}")

    periods = [ForecastPeriod(period=p["period"], forecast_value=p["forecast_value"]) for p in forecast_values]
    forecast = RollingForecast(
        name=name,
        metric=metric,
        frequency=frequency,
        horizon_periods=horizon_periods,
        periods=periods,
        current_period=periods[0].period if periods else "",
    )
    forecasts.append(forecast)
    logger.info("Rolling forecast created", forecast_id=forecast.id, name=name, metric=metric)
    return forecast


@app.get("/forecasts", response_model=List[RollingForecast])
async def list_forecasts(metric: Optional[str] = None):
    """List rolling forecasts."""
    if metric:
        return [f for f in forecasts if f.metric == metric]
    return forecasts


@app.get("/forecasts/{forecast_id}", response_model=RollingForecast)
async def get_forecast(forecast_id: str):
    """Get a specific forecast."""
    forecast = next((f for f in forecasts if f.id == forecast_id), None)
    if not forecast:
        raise HTTPException(status_code=404, detail="Forecast not found")
    return forecast


@app.post("/forecasts/{forecast_id}/update-actual")
async def update_actual(forecast_id: str, period: str, actual_value: float):
    """Update actual values and roll the forecast forward."""
    forecast = next((f for f in forecasts if f.id == forecast_id), None)
    if not forecast:
        raise HTTPException(status_code=404, detail="Forecast not found")

    period_entry = next((p for p in forecast.periods if p.period == period), None)
    if period_entry:
        period_entry.actual_value = actual_value
        period_entry.is_actual = True
        period_entry.variance = actual_value - period_entry.forecast_value
        period_entry.variance_pct = (
            (period_entry.variance / period_entry.forecast_value * 100) if period_entry.forecast_value else 0
        )

    forecast.last_updated = datetime.now(timezone.utc)
    logger.info("Actual updated", forecast_id=forecast_id, period=period, actual=actual_value)
    return {
        "forecast_id": forecast_id,
        "period": period,
        "actual_value": actual_value,
        "variance": period_entry.variance if period_entry else None,
    }


@app.post("/forecasts/{forecast_id}/roll")
async def roll_forecast(forecast_id: str, new_periods: List[Dict[str, Any]] = []):
    """Roll the forecast forward by adding new periods."""
    forecast = next((f for f in forecasts if f.id == forecast_id), None)
    if not forecast:
        raise HTTPException(status_code=404, detail="Forecast not found")

    for p in new_periods:
        forecast.periods.append(ForecastPeriod(period=p["period"], forecast_value=p["forecast_value"]))

    forecast.last_updated = datetime.now(timezone.utc)
    logger.info("Forecast rolled forward", forecast_id=forecast_id, new_periods=len(new_periods))
    return {"forecast_id": forecast_id, "total_periods": len(forecast.periods)}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
