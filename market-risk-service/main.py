"""
Vimbai Market Risk Service
Value at Risk (VaR), stress testing, and market exposure analysis.
Port: 8406
"""

import importlib.util
import math
import os
import sys
import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("market_risk_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["market_risk_calc"] = calc
_spec.loader.exec_module(calc)

SERVICE_NAME = "market-risk-service"
PORT = int(os.getenv("PORT", "8406"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Market Risk Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class Position(BaseModel):
    instrument: str
    exposure: float
    volatility: float = 0.15
    correlation: float = 1.0


class VaRRequest(BaseModel):
    company_id: str
    portfolio_name: str
    positions: List[Position]
    confidence_level: float = 0.95
    holding_period_days: int = 1


class VaRResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    portfolio_name: str
    total_exposure: float
    portfolio_volatility: float
    var_95: float
    var_99: float
    expected_shortfall: float
    stress_loss_2sd: float
    stress_loss_3sd: float
    risk_level: str


@app.get("/")
@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/var", response_model=VaRResult)
async def calculate_var(req: VaRRequest):
    result = calc.value_at_risk(
        req.company_id,
        req.portfolio_name,
        [p.model_dump() for p in req.positions],
        req.confidence_level,
        req.holding_period_days,
    )
    logger.info("VaR calculated", company=req.company_id, risk_level=result["risk_level"])
    return VaRResult(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
