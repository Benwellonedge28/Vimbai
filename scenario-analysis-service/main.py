"""Vimbai Scenario Analysis Service - best/base/worst case financial modeling. Port: 8372

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "scenario_analysis_service" not in _sys.modules or not hasattr(
    _sys.modules.get("scenario_analysis_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("scenario_analysis_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["scenario_analysis_service"] = _pkg
    _sys.modules["scenario_analysis_service"].__path__ = [_HERE]

import os

import structlog
from fastapi import Depends, FastAPI, Request
from neo4j import AsyncSession
from scenario_analysis_service import crud, models
from scenario_analysis_service.dependencies import book_id_var, get_db_session, get_user_id

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


def _calc_scenario(
    name: str,
    assumption: models.ScenarioAssumption,
    base_rev: float,
    base_cost: float,
    base_int: float,
    base_dep: float,
) -> models.ScenarioResult:
    rev = base_rev * (1 + assumption.revenue_growth)
    cost = base_cost * (1 + assumption.cost_growth)
    ebit = rev - cost - base_dep
    pretax = ebit - base_int
    tax = max(pretax, 0) * assumption.tax_rate
    net = pretax - tax
    margin = (net / rev * 100) if rev else 0
    return models.ScenarioResult(
        name=name,
        description=assumption.description,
        projected_revenue=round(rev, 2),
        projected_cost=round(cost, 2),
        ebit=round(ebit, 2),
        pretax_income=round(pretax, 2),
        net_income=round(net, 2),
        net_margin=round(margin, 2),
    )


@app.post("/analyze", response_model=models.AnalysisResponse)
async def analyze_scenarios(
    req: models.ScenarioRequest,
    user_id: str = Depends(get_user_id),
):
    """Pure computation over the submitted assumptions; nothing is persisted."""
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

    return models.AnalysisResponse(
        company_id=req.company_id,
        best_case=best,
        base_case=base,
        worst_case=worst,
        sensitivity_revenue=round(sens_rev, 2),
        sensitivity_cost=round(sens_cost, 2),
        recommendation=rec,
    )


@app.post("/scenarios", response_model=models.Scenario)
async def create_scenario(
    req: models.ScenarioCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    scenario = await crud.create_scenario(db_session, user_id, req)
    logger.info("scenario_created", company_id=scenario.company_id, name=scenario.name)
    return scenario


@app.get("/scenarios/{company_id}")
async def list_scenarios(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.list_scenarios(db_session, user_id, company_id)


@app.get("/compare/{company_id}")
async def compare_scenarios(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.compare_scenarios(db_session, user_id, company_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
