"""Vimbai Tax Planning Service - tax strategy records and planning calculations. Port: 8376

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "tax_planning_service" not in _sys.modules or not hasattr(_sys.modules.get("tax_planning_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("tax_planning_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["tax_planning_service"] = _pkg
    _sys.modules["tax_planning_service"].__path__ = [_HERE]

import os
from typing import List

import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from tax_planning_service import crud, models
from tax_planning_service.dependencies import book_id_var, get_db_session, get_user_id

SERVICE_NAME = "tax-planning-service"
PORT = int(os.getenv("PORT", "8376"))
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
app = FastAPI(title="Vimbai Tax Planning Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
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


@app.post("/strategies", response_model=models.TaxStrategy)
async def create_strategy(
    strategy: models.TaxStrategy,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_strategy(db_session, user_id, strategy)
    logger.info("strategy_created", name=item.name, type=item.strategy_type)
    return item


@app.get("/strategies", response_model=List[models.TaxStrategy])
async def list_strategies(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.list_strategies(db_session, user_id)


@app.post("/plan", response_model=models.PlanningResult)
async def create_plan(req: models.PlanningRequest):
    total_savings = sum(s.estimated_savings for s in req.strategies)
    total_cost = sum(s.implementation_cost for s in req.strategies)
    net_benefit = total_savings - total_cost
    projected_tax = max(req.current_tax - total_savings, 0)

    strategy_details = []
    recommended = []
    for s in req.strategies:
        roi = (
            (s.estimated_savings - s.implementation_cost) / s.implementation_cost
            if s.implementation_cost
            else float("inf")
        )
        strategy_details.append(
            {
                "id": s.id,
                "name": s.name,
                "type": s.strategy_type,
                "estimated_savings": s.estimated_savings,
                "implementation_cost": s.implementation_cost,
                "net_benefit": round(s.estimated_savings - s.implementation_cost, 2),
                "risk_level": s.risk_level,
                "timeframe": s.timeframe,
                "roi": round(roi, 2) if roi != float("inf") else None,
            }
        )
        if roi > 1 and s.risk_level in ("low", "medium"):
            recommended.append(s.name)

    return models.PlanningResult(
        company_id=req.company_id,
        fiscal_year=req.fiscal_year,
        current_tax=round(req.current_tax, 2),
        projected_tax=round(projected_tax, 2),
        total_savings=round(total_savings, 2),
        net_benefit=round(net_benefit, 2),
        strategies=strategy_details,
        recommended_strategies=recommended,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
