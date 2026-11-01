"""Vimbai Rolling Forecast Service
Creates rolling forecasts that update periodically with actual data.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "rolling_forecast_service" not in _sys.modules or not hasattr(
    _sys.modules.get("rolling_forecast_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("rolling_forecast_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["rolling_forecast_service"] = _pkg
    _sys.modules["rolling_forecast_service"].__path__ = [_HERE]
import os
from typing import Any, Dict, List, Optional

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from rolling_forecast_service import crud, models
from rolling_forecast_service.dependencies import book_id_var, get_db_session, get_user_id
from rolling_forecast_service.exceptions import NotFoundError, RollingForecastError

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(RollingForecastError)
async def _rolling_forecast_error(request: Request, exc: RollingForecastError):
    from fastapi.responses import JSONResponse

    status = 404 if isinstance(exc, NotFoundError) else 400
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/forecasts", response_model=models.RollingForecast)
async def create_forecast(
    name: str,
    metric: str,
    frequency: str = "monthly",
    horizon_periods: int = 12,
    forecast_values: List[Dict[str, Any]] = [],
    user_id: str = Depends(get_user_id),
    db_session=Depends(get_db_session),
):
    """Create a rolling forecast for the caller's Book."""
    if frequency not in models.VALID_FREQUENCIES:
        raise HTTPException(
            status_code=400,
            detail=f"Invalid frequency. Must be one of {models.VALID_FREQUENCIES}",
        )

    periods = [models.ForecastPeriod(period=p["period"], forecast_value=p["forecast_value"]) for p in forecast_values]
    forecast = models.RollingForecast(
        name=name,
        metric=metric,
        frequency=frequency,
        horizon_periods=horizon_periods,
        periods=periods,
        current_period=periods[0].period if periods else "",
    )
    created = await crud.create_forecast(db_session, user_id, forecast)
    logger.info("Rolling forecast created", forecast_id=created.id, name=name, metric=metric)
    return created


@app.get("/forecasts", response_model=List[models.RollingForecast])
async def list_forecasts(
    metric: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session=Depends(get_db_session),
):
    """List the caller's Book-visible forecasts."""
    return await crud.list_forecasts(db_session, user_id, metric)


@app.get("/forecasts/{forecast_id}", response_model=models.RollingForecast)
async def get_forecast(
    forecast_id: str,
    user_id: str = Depends(get_user_id),
    db_session=Depends(get_db_session),
):
    """Get a specific forecast; cross-scope reads 404."""
    return await crud.get_forecast(db_session, user_id, forecast_id)


@app.post("/forecasts/{forecast_id}/update-actual")
async def update_actual(
    forecast_id: str,
    period: str,
    actual_value: float,
    user_id: str = Depends(get_user_id),
    db_session=Depends(get_db_session),
):
    """Record an actual against a forecast period, keeping the original quirks."""
    forecast = await crud.get_forecast(db_session, user_id, forecast_id)

    period_entry = next((p for p in forecast.periods if p.period == period), None)
    if period_entry:
        period_entry.actual_value = actual_value
        period_entry.is_actual = True
        period_entry.variance = actual_value - period_entry.forecast_value
        period_entry.variance_pct = (
            (period_entry.variance / period_entry.forecast_value * 100) if period_entry.forecast_value else 0
        )

    await crud.save_periods(db_session, user_id, forecast, forecast.periods)
    logger.info("Actual updated", forecast_id=forecast_id, period=period, actual=actual_value)
    return {
        "forecast_id": forecast_id,
        "period": period,
        "actual_value": actual_value,
        "variance": period_entry.variance if period_entry else None,
    }


@app.post("/forecasts/{forecast_id}/roll")
async def roll_forecast(
    forecast_id: str,
    new_periods: List[Dict[str, Any]] = [],
    user_id: str = Depends(get_user_id),
    db_session=Depends(get_db_session),
):
    """Roll the forecast forward by appending periods."""
    forecast = await crud.get_forecast(db_session, user_id, forecast_id)

    for p in new_periods:
        forecast.periods.append(models.ForecastPeriod(period=p["period"], forecast_value=p["forecast_value"]))

    await crud.save_periods(db_session, user_id, forecast, forecast.periods)
    logger.info("Forecast rolled forward", forecast_id=forecast_id, new_periods=len(new_periods))
    return {"forecast_id": forecast_id, "total_periods": len(forecast.periods)}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
