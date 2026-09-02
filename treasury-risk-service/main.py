"""
Vimbai Treasury Risk Service
Manages treasury risk metrics: VaR, stress testing, and exposure limits.
"""

import math
import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "treasury-risk-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8259"))

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

app = FastAPI(title="Vimbai Treasury Risk Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class RiskExposure(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    exposure_type: str  # fx, interest_rate, credit, liquidity, commodity
    currency: str = "USD"
    notional_amount: float
    description: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class VaRResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    portfolio_value: float
    confidence_level: float  # e.g. 0.95, 0.99
    holding_period_days: int
    var_amount: float
    var_pct: float
    method: str = "parametric"  # parametric, historical, monte_carlo
    calculated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class StressTestScenario(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    shock_type: str  # interest_rate_up, interest_rate_down, fx_devaluation, market_crash
    shock_magnitude: float  # basis points or percentage
    portfolio_impact: float = 0.0
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class StressTestResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    scenario_id: str
    portfolio_value_before: float
    portfolio_value_after: float
    impact: float
    impact_pct: float
    tested_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


exposures: List[RiskExposure] = []
var_results: List[VaRResult] = []
scenarios: List[StressTestScenario] = []
stress_results: List[StressTestResult] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/exposures", response_model=RiskExposure)
async def create_exposure(exposure_type: str, currency: str, notional_amount: float, description: str = ""):
    """Register a risk exposure."""
    valid_types = ["fx", "interest_rate", "credit", "liquidity", "commodity"]
    if exposure_type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid type. Must be one of {valid_types}")

    exposure = RiskExposure(
        exposure_type=exposure_type,
        currency=currency,
        notional_amount=notional_amount,
        description=description,
    )
    exposures.append(exposure)
    logger.info("Risk exposure registered", exposure_id=exposure.id, type=exposure_type, notional=notional_amount)
    return exposure


@app.get("/exposures", response_model=List[RiskExposure])
async def list_exposures(exposure_type: Optional[str] = None):
    """List risk exposures."""
    if exposure_type:
        return [e for e in exposures if e.exposure_type == exposure_type]
    return exposures


@app.post("/var", response_model=VaRResult)
async def calculate_var(
    portfolio_value: float,
    confidence_level: float = 0.95,
    holding_period_days: int = 1,
    daily_volatility: float = 0.01,
    method: str = "parametric",
):
    """Calculate Value at Risk using parametric method."""
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
    var_results.append(result)
    logger.info("VaR calculated", var_id=result.id, var=var_amount, confidence=confidence_level)
    return result


@app.get("/var", response_model=List[VaRResult])
async def list_var_results(limit: int = 50):
    """List VaR results."""
    return var_results[-limit:]


@app.post("/scenarios", response_model=StressTestScenario)
async def create_scenario(name: str, description: str, shock_type: str, shock_magnitude: float):
    """Create a stress test scenario."""
    valid_types = ["interest_rate_up", "interest_rate_down", "fx_devaluation", "market_crash"]
    if shock_type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid shock type. Must be one of {valid_types}")

    scenario = StressTestScenario(
        name=name,
        description=description,
        shock_type=shock_type,
        shock_magnitude=shock_magnitude,
    )
    scenarios.append(scenario)
    logger.info("Stress test scenario created", scenario_id=scenario.id, name=name)
    return scenario


@app.get("/scenarios", response_model=List[StressTestScenario])
async def list_scenarios():
    """List stress test scenarios."""
    return scenarios


@app.post("/scenarios/{scenario_id}/run", response_model=StressTestResult)
async def run_stress_test(scenario_id: str, portfolio_value: float):
    """Run a stress test scenario on a portfolio."""
    scenario = next((s for s in scenarios if s.id == scenario_id), None)
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
    stress_results.append(result)
    logger.info("Stress test run", scenario_id=scenario_id, impact=impact, pct=impact_pct)
    return result


@app.get("/stress-results", response_model=List[StressTestResult])
async def list_stress_results(scenario_id: Optional[str] = None, limit: int = 50):
    """List stress test results."""
    result = stress_results
    if scenario_id:
        result = [r for r in result if r.scenario_id == scenario_id]
    return result[-limit:]


@app.get("/dashboard")
async def risk_dashboard():
    """Treasury risk dashboard summary."""
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
