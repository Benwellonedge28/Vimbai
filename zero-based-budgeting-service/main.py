"""Vimbai Zero-Based Budgeting Service - build budgets from zero each period. Port: 8328

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "zero_based_budgeting_service" not in _sys.modules or not hasattr(
    _sys.modules.get("zero_based_budgeting_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("zero_based_budgeting_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["zero_based_budgeting_service"] = _pkg
    _sys.modules["zero_based_budgeting_service"].__path__ = [_HERE]

import os

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from neo4j import AsyncSession
from zero_based_budgeting_service import crud, models
from zero_based_budgeting_service.dependencies import book_id_var, get_db_session, get_user_id
from zero_based_budgeting_service.exceptions import ValidationError, ZeroBasedBudgetingError

SERVICE_NAME = "zero-based-budgeting-service"
PORT = int(os.getenv("PORT", "8328"))
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
app = FastAPI(title="Vimbai Zero-Based Budgeting Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(ZeroBasedBudgetingError)
async def _zbb_error(request: Request, exc: ZeroBasedBudgetingError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/packages", response_model=models.ZBBPackage)
async def create_package(
    pkg: models.ZBBPackageCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    created = await crud.create_package(db_session, user_id, pkg)
    logger.info(
        "zbb_package_created",
        company_id=created.company_id,
        department=created.department,
        total=created.total_amount,
    )
    return created


@app.get("/packages/{company_id}")
async def get_packages(
    company_id: str,
    department: str = None,
    status_filter: str = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.get_packages(db_session, user_id, company_id, department or "", status_filter or "")


@app.post("/packages/{package_id}/items")
async def add_item(
    package_id: str,
    item: models.BudgetItem,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.add_item(db_session, user_id, package_id, item)
    except ZeroBasedBudgetingError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.put("/packages/{package_id}/status")
async def update_status(
    package_id: str,
    status: models.ZBBStatus,
    reviewer: str = "",
    notes: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.update_status(db_session, user_id, package_id, status, reviewer, notes)
    except ZeroBasedBudgetingError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.put("/items/{item_id}/priority")
async def set_item_priority(
    item_id: str,
    priority: int,
    status: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.set_item_priority(db_session, user_id, item_id, priority, status)
    except ValidationError as exc:
        raise HTTPException(status_code=400, detail=str(exc))  # original semantics: 400
    except ZeroBasedBudgetingError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.get("/summary/{company_id}")
async def zbb_summary(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.zbb_summary(db_session, user_id, company_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
