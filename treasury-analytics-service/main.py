"""Vimbai Treasury Analytics Service - Analytics and KPIs for treasury operations. Port: 8322

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "treasury_analytics_service" not in _sys.modules or not hasattr(
    _sys.modules.get("treasury_analytics_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("treasury_analytics_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["treasury_analytics_service"] = _pkg
    _sys.modules["treasury_analytics_service"].__path__ = [_HERE]

import os
from decimal import ROUND_HALF_EVEN, Decimal, getcontext, localcontext

import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from treasury_analytics_service import crud, engine, models
from treasury_analytics_service.dependencies import book_id_var, get_db_session, get_user_id

SERVICE_NAME = "treasury-analytics-service"
PORT = int(os.getenv("PORT", "8322"))
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

# Vimbai holds any figure at any magnitude: the default 28-digit decimal
# context cannot even represent a 30-digit balance.
getcontext().prec = 200
getcontext().rounding = ROUND_HALF_EVEN


app = FastAPI(title="Vimbai Treasury Analytics Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
# Distributed tracing (OpenTelemetry)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="treasury-analytics-service", instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "calc_engine": engine.engine_in_use()}


@app.post("/analyze", response_model=models.AnalyticsResponse)
async def analyze_treasury(
    req: models.AnalyticsRequest,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Compute treasury KPIs (pure calculation) and persist the snapshot for the caller's Book."""
    resp = engine.compute(req)
    await crud.save_snapshot(db_session, user_id, resp)
    logger.info(
        "treasury_analyzed",
        company_id=req.company_id,
        cash_adequacy_days=resp.cash_adequacy_days,
        debt_service_ratio=resp.debt_service_ratio,
        fx_risk_score=resp.fx_risk_score,
        calc_engine=engine.engine_in_use(),
    )
    return resp


@app.get("/kpi/{company_id}")
async def get_kpis(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Return the caller's latest stored KPI snapshot for the company."""
    snapshot = await crud.get_snapshot(db_session, user_id, company_id)
    if snapshot:
        return snapshot
    return {"company_id": company_id, "message": "Run /analyze first"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
