"""Vimbai Job Costing Service - Track costs per job/project. Port: 8361

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "job_costing_service" not in _sys.modules or not hasattr(_sys.modules.get("job_costing_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("job_costing_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["job_costing_service"] = _pkg
    _sys.modules["job_costing_service"].__path__ = [_HERE]

import os
from typing import Optional

import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from job_costing_service import crud, models
from job_costing_service.dependencies import book_id_var, get_db_session, get_user_id
from job_costing_service.exceptions import NotFoundError
from neo4j import AsyncSession

SERVICE_NAME = "job-costing-service"
PORT = int(os.getenv("PORT", "8361"))
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
app = FastAPI(title="Vimbai Job Costing Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="job-costing-service", instrument_app=app)
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


@app.post("/jobs", response_model=models.Job)
async def create_job(
    job: models.JobCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_job(db_session, user_id, job)
    logger.info("job_created", company_id=item.company_id, job_id=item.id)
    return item


@app.get("/jobs/{company_id}")
async def get_jobs(
    company_id: str,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    jobs = await crud.get_jobs(db_session, user_id, company_id, status)
    return {"company_id": company_id, "jobs": jobs, "total": len(jobs)}


@app.post("/jobs/{job_id}/costs")
async def add_cost(
    job_id: str,
    entry: models.JobCostEntryCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    result = await crud.add_cost(db_session, user_id, job_id, entry)
    logger.info("job_cost_added", job_id=job_id, cost_type=entry.cost_type, amount=entry.amount)
    return result


@app.get("/jobs/{company_id}/profitability")
async def job_profitability(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.job_profitability(db_session, user_id, company_id)


@app.exception_handler(NotFoundError)
async def not_found_handler(request: Request, exc: NotFoundError):
    from fastapi.responses import JSONResponse

    return JSONResponse(status_code=404, content={"detail": exc.detail})


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
