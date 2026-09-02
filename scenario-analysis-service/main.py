"""
Vimbai Scenario Analysis Service
Best/base/worst case financial modeling and sensitivity analysis.
Port: 8372
"""

import os
import uuid
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "scenario-analysis-service"
PORT = int(os.getenv("PORT", "8372"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Scenario Analysis Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class ScenarioAssumption(BaseModel):
    revenue_growth: float = 0.1
    cost_growth: float = 0.05
    interest_rate: float = 0.05
    tax_rate: float = 0.25
    capex: float = 0
    description: str = ""


class ScenarioRequest(BaseModel):
    company_id: str
    base_revenue: float
    base_cost: float
    base_interest: float = 0
    base_depreciation: float = 0
    best_case: ScenarioAssumption
    base_case: ScenarioAssumption
    worst_case: ScenarioAssumption


class ScenarioResult(BaseModel):
    name: str
    description: str
    projected_revenue: float
    projected_cost: float
    ebit: float
    pretax_income: float
    net_income: float
    net_margin: float


class AnalysisResponse(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    best_case: ScenarioResult
    base_case: ScenarioResult
    worst_case: ScenarioResult
    sensitivity_revenue: float
    sensitivity_cost: float
    recommendation: str


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


def _calc_scenario(
    name: str, assumption: ScenarioAssumption, base_rev: float, base_cost: float, base_int: float, base_dep: float
) -> ScenarioResult:
    rev = base_rev * (1 + assumption.revenue_growth)
    cost = base_cost * (1 + assumption.cost_growth)
    ebit = rev - cost - base_dep
    pretax = ebit - base_int
    tax = max(pretax, 0) * assumption.tax_rate
    net = pretax - tax
    margin = (net / rev * 100) if rev else 0
    return ScenarioResult(
        name=name,
        description=assumption.description,
        projected_revenue=round(rev, 2),
        projected_cost=round(cost, 2),
        ebit=round(ebit, 2),
        pretax_income=round(pretax, 2),
        net_income=round(net, 2),
        net_margin=round(margin, 2),
    )


@app.post("/analyze", response_model=AnalysisResponse)
async def analyze_scenarios(req: ScenarioRequest):
    best = _calc_scenario(
        "Best Case", req.best_case, req.base_revenue, req.base_cost, req.base_interest, req.base_depreciation
    )
    base = _calc_scenario(
        "Base Case", req.base_case, req.base_revenue, req.base_cost, req.base_interest, req.base_depreciation
    )
    worst = _calc_scenario(
        "Worst Case", req.worst_case, req.base_revenue, req.base_cost, req.base_interest, req.base_depreciation
    )

    sens_rev = best.net_income - worst.net_income
    sens_cost = base.net_income - worst.net_income

    if worst.net_income > 0:
        rec = "All scenarios profitable - proceed with current strategy"
    elif base.net_income > 0:
        rec = "Base case profitable but worst case shows losses - implement cost controls"
    else:
        rec = "Base case unprofitable - immediate restructuring required"

    return AnalysisResponse(
        company_id=req.company_id,
        best_case=best,
        base_case=base,
        worst_case=worst,
        sensitivity_revenue=round(sens_rev, 2),
        sensitivity_cost=round(sens_cost, 2),
        recommendation=rec,
    )


# Backward-compatible /scenarios endpoints (for platform test compatibility)
_scenarios_store: Dict[str, List[Dict]] = {}


class ScenarioCreate(BaseModel):
    company_id: str
    name: str
    scenario_type: str = "custom"
    projected_revenue: float = 0
    projected_expenses: float = 0


@app.post("/scenarios", response_model=dict)
async def create_scenario(req: ScenarioCreate):
    sid = str(uuid.uuid4())
    scenario = {
        "id": sid,
        "company_id": req.company_id,
        "name": req.name,
        "scenario_type": req.scenario_type,
        "projected_revenue": req.projected_revenue,
        "projected_expenses": req.projected_expenses,
        "net_projection": req.projected_revenue - req.projected_expenses,
    }
    _scenarios_store.setdefault(req.company_id, []).append(scenario)
    return scenario


@app.get("/scenarios/{company_id}")
async def list_scenarios(company_id: str):
    items = _scenarios_store.get(company_id, [])
    return {"total": len(items), "scenarios": items}


@app.get("/compare/{company_id}")
async def compare_scenarios(company_id: str):
    items = _scenarios_store.get(company_id, [])
    if len(items) < 2:
        return {"comparison": "Need at least 2 scenarios", "best_case": "", "worst_case": ""}
    best = max(items, key=lambda s: s["net_projection"])
    worst = min(items, key=lambda s: s["net_projection"])
    return {
        "best_case": best["name"],
        "worst_case": worst["name"],
        "best_net": best["net_projection"],
        "worst_net": worst["net_projection"],
        "range": best["net_projection"] - worst["net_projection"],
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
