"""
Vimbai Treasury Risk Service
Manages treasury risk metrics: VaR, stress testing, and exposure limits.
Caller-owned (X-User-Id) and Book-gated (X-Book-ID) stores persist in
Neo4j; VaR and stress-impact math stay pure computation.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import logging
import math
import os as _os
import sys as _sys
from datetime import datetime
from typing import List, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "treasury_risk_service" not in _sys.modules or not hasattr(_sys.modules.get("treasury_risk_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("treasury_risk_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["treasury_risk_service"] = _pkg
    _sys.modules["treasury_risk_service"].__path__ = [_HERE]

from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from treasury_risk_service import crud
from treasury_risk_service.dependencies import book_id_var, get_db_session, get_user_id
from treasury_risk_service.models import RiskExposure, StressTestResult, StressTestScenario, VaRResult

SERVICE_NAME = "treasury-risk-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8259"))

try:
    import structlog

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
except ImportError:  # pragma: no cover
    logging.basicConfig(level=logging.INFO)
    logger = logging.getLogger(SERVICE_NAME)

app = FastAPI(title="Vimbai Treasury Risk Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/exposures", response_model=RiskExposure)
async def create_exposure(
    exposure_type: str,
    currency: str,
    notional_amount: float,
    description: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Register a risk exposure (caller-scoped)."""
    valid_types = ["fx", "interest_rate", "credit", "liquidity", "commodity"]
    if exposure_type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid type. Must be one of {valid_types}")

    exposure = RiskExposure(
        exposure_type=exposure_type,
        currency=currency,
        notional_amount=notional_amount,
        description=description,
    )
    saved = await crud.create_exposure(db_session, user_id, exposure)
    logger.info("Risk exposure registered", exposure_id=saved.id, type=exposure_type, notional=notional_amount)
    return saved


@app.get("/exposures", response_model=List[RiskExposure])
async def list_exposures(
    exposure_type: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List risk exposures (caller's own Book-visible set)."""
    result = await crud.list_exposures(db_session, user_id)
    if exposure_type:
        result = [e for e in result if e.exposure_type == exposure_type]
    return result


@app.post("/var", response_model=VaRResult)
async def calculate_var(
    portfolio_value: float,
    confidence_level: float = 0.95,
    holding_period_days: int = 1,
    daily_volatility: float = 0.01,
    method: str = "parametric",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate Value at Risk using parametric method (persisted to the caller's set)."""
    valid_confidences = [0.90, 0.95, 0.99]
    if confidence_level not in valid_confidences:
        raise HTTPException(status_code=400, detail=f"Confidence must be one of {valid_confidences}")

    # Z-scores for common confidence levels
    z_scores = {0.90: 1.282, 0.95: 1.645, 0.99: 2.326}
    z = z_scores[confidence_level]

    var_amount = portfolio_value * z * daily_volatility * math.sqrt(holding_period_days)
    var_pct = (var_amount / portfolio_value * 100) if portfolio_value else 0.0

    result = VaRResult(
        portfolio_value=portfolio_value,
        confidence_level=confidence_level,
        holding_period_days=holding_period_days,
        var_amount=round(var_amount, 2),
        var_pct=round(var_pct, 4),
        method=method,
    )
    saved = await crud.create_var(db_session, user_id, result)
    logger.info("VaR calculated", var_id=saved.id, var=var_amount, confidence=confidence_level)
    return saved


@app.get("/var", response_model=List[VaRResult])
async def list_var_results(
    limit: int = 50,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List VaR results (caller's own Book-visible set)."""
    result = await crud.list_var_results(db_session, user_id)
    # original semantics: last N entries in insertion order
    return result[-limit:]


@app.post("/scenarios", response_model=StressTestScenario)
async def create_scenario(
    name: str,
    description: str,
    shock_type: str,
    shock_magnitude: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a stress test scenario (caller-scoped)."""
    valid_types = ["interest_rate_up", "interest_rate_down", "fx_devaluation", "market_crash"]
    if shock_type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid shock type. Must be one of {valid_types}")

    scenario = StressTestScenario(
        name=name,
        description=description,
        shock_type=shock_type,
        shock_magnitude=shock_magnitude,
    )
    saved = await crud.create_scenario(db_session, user_id, scenario)
    logger.info("Stress test scenario created", scenario_id=saved.id, name=name)
    return saved


@app.get("/scenarios", response_model=List[StressTestScenario])
async def list_scenarios(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List stress test scenarios (caller's own Book-visible set)."""
    return await crud.list_scenarios(db_session, user_id)


@app.post("/scenarios/{scenario_id}/run", response_model=StressTestResult)
async def run_stress_test(
    scenario_id: str,
    portfolio_value: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Run a stress test scenario on a portfolio (caller's own scenarios only)."""
    scenario = await crud.get_scenario(db_session, user_id, scenario_id)
    if not scenario:
        raise HTTPException(status_code=404, detail="Scenario not found")

    # Calculate impact based on shock type
    if scenario.shock_type == "market_crash":
        impact = -portfolio_value * (scenario.shock_magnitude / 100)
    elif scenario.shock_type == "fx_devaluation":
        impact = -portfolio_value * (scenario.shock_magnitude / 100)
    elif scenario.shock_type in ("interest_rate_up", "interest_rate_down"):
        # Duration-based impact: simplified
        impact = -portfolio_value * (scenario.shock_magnitude / 10000) * 5  # 5-year duration assumption
    else:
        impact = 0.0

    portfolio_after = portfolio_value + impact
    impact_pct = (impact / portfolio_value * 100) if portfolio_value else 0.0

    result = StressTestResult(
        scenario_id=scenario_id,
        portfolio_value_before=portfolio_value,
        portfolio_value_after=round(portfolio_after, 2),
        impact=round(impact, 2),
        impact_pct=round(impact_pct, 4),
    )
    saved = await crud.create_stress_result(db_session, user_id, result)
    logger.info("Stress test run", scenario_id=scenario_id, impact=impact, pct=impact_pct)
    return saved


@app.get("/stress-results", response_model=List[StressTestResult])
async def list_stress_results(
    scenario_id: Optional[str] = None,
    limit: int = 50,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List stress test results (caller's own Book-visible set)."""
    result = await crud.list_stress_results(db_session, user_id)
    if scenario_id:
        result = [r for r in result if r.scenario_id == scenario_id]
    # original semantics: last N entries in insertion order
    return result[-limit:]


@app.get("/dashboard")
async def risk_dashboard(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Treasury risk dashboard summary (over the caller's Book-visible set)."""
    exposures = await crud.list_exposures(db_session, user_id)
    var_results = await crud.list_var_results(db_session, user_id)
    scenarios = await crud.list_scenarios(db_session, user_id)
    stress_results = await crud.list_stress_results(db_session, user_id)

    return {
        "total_exposures": len(exposures),
        "total_notional": sum(e.notional_amount for e in exposures),
        "by_type": {
            t: sum(e.notional_amount for e in exposures if e.exposure_type == t)
            for t in set(e.exposure_type for e in exposures)
        },
        "var_calculations": len(var_results),
        "latest_var": var_results[-1].var_amount if var_results else 0,
        "stress_scenarios": len(scenarios),
        "stress_tests_run": len(stress_results),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
