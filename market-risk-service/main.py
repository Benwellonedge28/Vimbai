"""
Vimbai Market Risk Service
Value at Risk (VaR), stress testing, and market exposure analysis.
Port: 8406
"""
import os, uuid, math
from datetime import datetime, timezone
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "market-risk-service"
PORT = int(os.getenv("PORT", "8406"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Market Risk Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class Position(BaseModel):
    instrument: str; exposure: float; volatility: float = 0.15
    correlation: float = 1.0

class VaRRequest(BaseModel):
    company_id: str; portfolio_name: str
    positions: List[Position]
    confidence_level: float = 0.95
    holding_period_days: int = 1

class VaRResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; portfolio_name: str
    total_exposure: float; portfolio_volatility: float
    var_95: float; var_99: float
    expected_shortfall: float
    stress_loss_2sd: float; stress_loss_3sd: float
    risk_level: str

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/var", response_model=VaRResult)
async def calculate_var(req: VaRRequest):
    total = sum(p.exposure for p in req.positions)
    
    # Simple portfolio volatility (assuming diversification benefit)
    weighted_vol = sum(p.exposure * p.volatility for p in req.positions) / total if total else 0
    portfolio_vol = weighted_vol * 0.85  # diversification approximation
    
    z_95 = 1.645; z_99 = 2.326
    var_95 = total * portfolio_vol * z_95 * math.sqrt(req.holding_period_days)
    var_99 = total * portfolio_vol * z_99 * math.sqrt(req.holding_period_days)
    es = total * portfolio_vol * 2.063 * math.sqrt(req.holding_period_days)  # ES at 95%
    
    stress_2sd = total * portfolio_vol * 2
    stress_3sd = total * portfolio_vol * 3
    
    if var_95 / total > 0.20:
        risk = "high"
    elif var_95 / total > 0.10:
        risk = "medium"
    else:
        risk = "low"
    
    return VaRResult(
        company_id=req.company_id, portfolio_name=req.portfolio_name,
        total_exposure=round(total, 2),
        portfolio_volatility=round(portfolio_vol, 4),
        var_95=round(var_95, 2), var_99=round(var_99, 2),
        expected_shortfall=round(es, 2),
        stress_loss_2sd=round(stress_2sd, 2),
        stress_loss_3sd=round(stress_3sd, 2),
        risk_level=risk
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
