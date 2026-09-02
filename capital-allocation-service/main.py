"""
Vimbai Capital Allocation Service
Capital budgeting, project ranking, and resource allocation optimization.
Port: 8394
"""
import os, uuid, math
from typing import Dict, List
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "capital-allocation-service"
PORT = int(os.getenv("PORT", "8394"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Capital Allocation Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class Project(BaseModel):
    name: str; initial_investment: float
    annual_cash_flows: List[float]; discount_rate: float = 0.10
    strategic_value: int = 5  # 1-10

class AllocationRequest(BaseModel):
    company_id: str; capital_budget: float
    projects: List[Project]

class AllocationResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; capital_budget: float
    selected_projects: List[Dict]
    rejected_projects: List[Dict]
    total_investment: float; total_npv: float
    utilization_pct: float; ranking_method: str

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

def _calc_npv(initial, cashflows, rate):
    npv = -initial
    for i, cf in enumerate(cashflows):
        npv += cf / (1 + rate) ** (i + 1)
    return npv

def _calc_irr(initial, cashflows):
    for r in [0.05, 0.10, 0.15, 0.20, 0.25, 0.30, 0.40, 0.50]:
        npv = _calc_npv(initial, cashflows, r)
        if npv <= 0:
            return r
    return 0.50

@app.post("/allocate", response_model=AllocationResult)
async def allocate_capital(req: AllocationRequest):
    scored = []
    for p in req.projects:
        npv = _calc_npv(p.initial_investment, p.annual_cash_flows, p.discount_rate)
        irr = _calc_irr(p.initial_investment, p.annual_cash_flows)
        profitability_index = (npv + p.initial_investment) / p.initial_investment if p.initial_investment else 0
        score = npv * 0.6 + p.strategic_value * 10000 * 0.4
        scored.append({
            "name": p.name, "initial_investment": p.initial_investment,
            "npv": round(npv, 2), "irr": round(irr * 100, 1),
            "profitability_index": round(profitability_index, 3),
            "strategic_value": p.strategic_value, "score": round(score, 2)
        })
    
    scored.sort(key=lambda x: x["score"], reverse=True)
    
    selected = []; rejected = []; invested = 0
    for p in scored:
        if invested + p["initial_investment"] <= req.capital_budget:
            selected.append(p)
            invested += p["initial_investment"]
        else:
            rejected.append(p)
    
    total_npv = sum(p["npv"] for p in selected)
    utilization = invested / req.capital_budget * 100 if req.capital_budget else 0
    
    return AllocationResult(
        company_id=req.company_id, capital_budget=round(req.capital_budget, 2),
        selected_projects=selected, rejected_projects=rejected,
        total_investment=round(invested, 2), total_npv=round(total_npv, 2),
        utilization_pct=round(utilization, 1),
        ranking_method="NPV_weighted_with_strategic_value"
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
