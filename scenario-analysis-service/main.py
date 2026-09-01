"""Vimbai Scenario Analysis Service - Multi-scenario financial modeling. Port: 8362"""
import os, uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "scenario-analysis-service"
PORT = int(os.getenv("PORT", "8362"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Scenario Analysis Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="scenario-analysis-service", instrument_app=app)
except ImportError:
    TRACER = None

class ScenarioType(str, Enum):
    OPTIMISTIC = "optimistic"; BASE = "base"; PESSIMISTIC = "pessimistic"; CUSTOM = "custom"

class ScenarioAssumption(BaseModel):
    variable: str
    value: float

class Scenario(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    name: str
    scenario_type: ScenarioType = ScenarioType.BASE
    assumptions: List[ScenarioAssumption] = []
    projected_revenue: float = 0
    projected_expenses: float = 0
    projected_profit: float = 0
    projected_cash_flow: float = 0
    notes: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))

_scenarios: Dict[str, List[Scenario]] = defaultdict(list)

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/scenarios", response_model=Scenario)
async def create_scenario(sc: Scenario):
    sc.projected_profit = sc.projected_revenue - sc.projected_expenses
    sc.projected_cash_flow = sc.projected_profit * 1.1  # simplified: add back non-cash items
    _scenarios[sc.company_id].append(sc)
    logger.info("scenario_created", company_id=sc.company_id, type=sc.scenario_type.value, profit=sc.projected_profit)
    return sc

@app.get("/scenarios/{company_id}")
async def get_scenarios(company_id: str, scenario_type: Optional[str] = None):
    scen = _scenarios.get(company_id, [])
    if scenario_type: scen = [s for s in scen if s.scenario_type.value == scenario_type]
    return {"company_id": company_id, "scenarios": scen, "total": len(scen)}

@app.get("/compare/{company_id}")
async def compare_scenarios(company_id: str):
    scen = _scenarios.get(company_id, [])
    if len(scen) < 2: raise HTTPException(status_code=400, detail="Need at least 2 scenarios to compare")
    comparison = {"company_id": company_id, "scenarios": [{"name": s.name, "type": s.scenario_type.value, "revenue": s.projected_revenue, "expenses": s.projected_expenses, "profit": s.projected_profit, "cash_flow": s.projected_cash_flow} for s in scen]}
    comparison["best_case"] = max(scen, key=lambda s: s.projected_profit).name
    comparison["worst_case"] = min(scen, key=lambda s: s.projected_profit).name
    comparison["range"] = max(s.projected_profit for s in scen) - min(s.projected_profit for s in scen)
    return comparison

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
