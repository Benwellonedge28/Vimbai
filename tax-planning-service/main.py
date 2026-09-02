"""
Vimbai Tax Planning Service
Tax strategy optimization, scenario modeling, and savings identification.
Port: 8376
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "tax-planning-service"
PORT = int(os.getenv("PORT", "8376"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Tax Planning Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class TaxStrategy(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str; description: str; strategy_type: str  # deduction, credit, timing, structure, treaty
    estimated_savings: float; implementation_cost: float = 0
    risk_level: str = "low"; timeframe: str = "short-term"

class PlanningRequest(BaseModel):
    company_id: str; fiscal_year: int
    current_taxable_income: float; current_tax: float
    strategies: List[TaxStrategy] = []

class PlanningResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; fiscal_year: int
    current_tax: float; projected_tax: float; total_savings: float
    net_benefit: float; strategies: List[Dict]
    recommended_strategies: List[str] = []

_strategies: Dict[str, List[TaxStrategy]] = {}

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/strategies", response_model=TaxStrategy)
async def create_strategy(strategy: TaxStrategy):
    _strategies.setdefault(strategy.id, []).append(strategy)
    return strategy

@app.post("/plan", response_model=PlanningResult)
async def create_plan(req: PlanningRequest):
    total_savings = sum(s.estimated_savings for s in req.strategies)
    total_cost = sum(s.implementation_cost for s in req.strategies)
    net_benefit = total_savings - total_cost
    projected_tax = max(req.current_tax - total_savings, 0)
    
    strategy_details = []
    recommended = []
    for s in req.strategies:
        roi = (s.estimated_savings - s.implementation_cost) / s.implementation_cost if s.implementation_cost else float('inf')
        strategy_details.append({
            "id": s.id, "name": s.name, "type": s.strategy_type,
            "estimated_savings": s.estimated_savings,
            "implementation_cost": s.implementation_cost,
            "net_benefit": round(s.estimated_savings - s.implementation_cost, 2),
            "risk_level": s.risk_level, "timeframe": s.timeframe,
            "roi": round(roi, 2) if roi != float('inf') else None
        })
        if roi > 1 and s.risk_level in ("low", "medium"):
            recommended.append(s.name)
    
    return PlanningResult(
        company_id=req.company_id, fiscal_year=req.fiscal_year,
        current_tax=round(req.current_tax, 2),
        projected_tax=round(projected_tax, 2),
        total_savings=round(total_savings, 2),
        net_benefit=round(net_benefit, 2),
        strategies=strategy_details, recommended_strategies=recommended
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
