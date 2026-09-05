"""Vimbai Risk Mitigation Service - Risk management and investigation. Port: 8330

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "risk_mitigation_service" not in _sys.modules or not hasattr(
    _sys.modules.get("risk_mitigation_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("risk_mitigation_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["risk_mitigation_service"] = _pkg
    _sys.modules["risk_mitigation_service"].__path__ = [_HERE]

import os
import uuid
from typing import Optional

import structlog
from fastapi import Depends, FastAPI, Header, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from risk_mitigation_service import crud, models
from risk_mitigation_service.dependencies import book_id_var, get_db_session, get_user_id
from risk_mitigation_service.exceptions import NotFoundError
from risk_mitigation_service.models import RiskCategory, RiskLevel

SERVICE_NAME = "risk-mitigation-service"
PORT = int(os.getenv("PORT", "8331"))
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
app = FastAPI(title="Vimbai Risk Mitigation Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="risk-mitigation-service", instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/risks")
async def create_risk(
    risk: models.RiskItemCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_risk(db_session, user_id, risk)
    logger.info("risk_created", company_id=item.company_id, category=item.category.value, level=item.level.value)
    return {"id": item.id, "risk_score": item.risk_score, "level": item.level.value}


@app.get("/risks/{company_id}")
async def get_risks(
    company_id: str,
    category: Optional[str] = None,
    level: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    risks = await crud.get_risks(db_session, user_id, company_id, category, level)
    return {
        "company_id": company_id,
        "risks": risks,
        "total": len(risks),
        "by_level": {l.value: sum(1 for r in risks if r.level == l) for l in RiskLevel},
    }


@app.put("/risks/{risk_id}")
async def update_risk(
    risk_id: str,
    likelihood: Optional[int] = None,
    impact: Optional[int] = None,
    mitigation: Optional[str] = None,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.update_risk(db_session, user_id, risk_id, likelihood, impact, mitigation, status)
    return {"id": item.id, "risk_score": item.risk_score, "level": item.level.value, "status": item.status}


@app.get("/dashboard/{company_id}")
async def risk_dashboard(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    risks = await crud.get_risks(db_session, user_id, company_id)
    if not risks:
        return {
            "company_id": company_id,
            "total_risks": 0,
            "by_level": {},
            "by_category": {},
            "avg_score": 0,
            "top_risks": [],
        }
    by_level = {l.value: sum(1 for r in risks if r.level.value == l.value) for l in RiskLevel}
    by_category = {c.value: sum(1 for r in risks if r.category.value == c.value) for c in RiskCategory}
    avg = sum(r.risk_score for r in risks) / len(risks)
    top = sorted(risks, key=lambda r: r.risk_score, reverse=True)[:5]
    return {
        "company_id": company_id,
        "total_risks": len(risks),
        "by_level": by_level,
        "by_category": by_category,
        "avg_score": avg,
        "top_risks": top,
    }


@app.delete("/risks/{risk_id}")
async def close_risk(
    risk_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.close_risk(db_session, user_id, risk_id)
    return {"id": item.id, "status": item.status}


@app.exception_handler(NotFoundError)
async def not_found_handler(request: Request, exc: NotFoundError):
    from fastapi.responses import JSONResponse

    return JSONResponse(status_code=404, content={"detail": exc.detail})


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
