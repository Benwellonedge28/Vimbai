"""Vimbai Sensitivity Analysis Service - what-if analysis on financial variables. Port: 8325

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "sensitivity_analysis_service" not in _sys.modules or not hasattr(
    _sys.modules.get("sensitivity_analysis_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("sensitivity_analysis_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["sensitivity_analysis_service"] = _pkg
    _sys.modules["sensitivity_analysis_service"].__path__ = [_HERE]

import os
from collections import defaultdict

import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from sensitivity_analysis_service import crud, models
from sensitivity_analysis_service.dependencies import book_id_var, get_db_session, get_user_id

SERVICE_NAME = "sensitivity-analysis-service"
PORT = int(os.getenv("PORT", "8325"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Sensitivity Analysis Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="sensitivity-analysis-service", instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


def estimate_impact(var: models.Variable, target: str, base_target: float) -> float:
    """Estimate impact of variable change on target metric using simplified linear model."""
    change_pct = var.change_pct / 100
    if target in ("net_profit", "profit"):
        if "revenue" in var.name.lower():
            return base_target * (1 + change_pct * 0.6)
        elif "cost" in var.name.lower() or "expense" in var.name.lower():
            return base_target * (1 - change_pct * 0.4)
        elif "interest" in var.name.lower():
            return base_target * (1 - change_pct * 0.1)
        return base_target * (1 + change_pct * 0.2)
    elif target in ("cash_flow", "cashflow"):
        if "revenue" in var.name.lower():
            return base_target * (1 + change_pct * 0.7)
        elif "cost" in var.name.lower():
            return base_target * (1 - change_pct * 0.5)
        return base_target * (1 + change_pct * 0.3)
    elif target == "revenue":
        if "price" in var.name.lower():
            return base_target * (1 + change_pct * 0.8)
        elif "volume" in var.name.lower() or "sales" in var.name.lower():
            return base_target * (1 + change_pct * 0.9)
        return base_target * (1 + change_pct * 0.3)
    return base_target * (1 + change_pct * 0.2)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/analyze", response_model=models.AnalysisResponse)
async def run_analysis(
    req: models.AnalysisRequest,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    results = []
    for var in req.variables:
        for step in req.change_steps:
            var_copy = models.Variable(name=var.name, base_value=var.base_value, change_pct=step)
            changed_value = var.base_value * (1 + step / 100)
            impact = estimate_impact(var_copy, req.target_metric, req.base_target_value)
            elasticity = (
                (step / 100) / ((impact - req.base_target_value) / max(1, req.base_target_value))
                if impact != req.base_target_value
                else 0
            )
            results.append(
                models.SensitivityResult(
                    variable_name=var.name,
                    base_value=var.base_value,
                    changed_value=changed_value,
                    change_pct=step,
                    impact_on_target=impact,
                    elasticity=abs(elasticity),
                )
            )

    # Find most sensitive variable (highest avg elasticity)
    var_elasticity = defaultdict(list)
    for r in results:
        var_elasticity[r.variable_name].append(r.elasticity)
    most_sensitive = max(var_elasticity, key=lambda v: sum(var_elasticity[v]) / len(var_elasticity[v]), default="")

    resp = models.AnalysisResponse(
        user_id=user_id,
        book_id=book_id_var.get(),
        company_id=req.company_id,
        target_metric=req.target_metric,
        base_target_value=req.base_target_value,
        results=results,
        most_sensitive_variable=most_sensitive,
    )
    await crud.store_analysis(db_session, user_id, resp)
    return resp


@app.get("/analyses/{company_id}")
async def get_analyses(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.get_analyses(db_session, user_id, company_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
