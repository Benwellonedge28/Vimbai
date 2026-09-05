"""Vimbai Balance Sheet Service - Generate and manage balance sheets. Port: 8345

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "balance_sheet_service" not in _sys.modules or not hasattr(_sys.modules.get("balance_sheet_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("balance_sheet_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["balance_sheet_service"] = _pkg
    _sys.modules["balance_sheet_service"].__path__ = [_HERE]

import os

import structlog
from balance_sheet_service import crud, models
from balance_sheet_service.dependencies import book_id_var, get_db_session, get_user_id
from balance_sheet_service.exceptions import NotFoundError
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "balance-sheet-service"
PORT = int(os.getenv("PORT", "8345"))
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
app = FastAPI(title="Vimbai Balance Sheet Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="balance-sheet-service", instrument_app=app)
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


@app.post("/generate", response_model=models.BalanceSheet)
async def generate_balance_sheet(
    sheet: models.BalanceSheetCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.generate_sheet(db_session, user_id, sheet)
    if not item.is_balanced:
        logger.warning(
            "balance_sheet_unbalanced",
            company_id=item.company_id,
            diff=item.total_assets - (item.total_liabilities + item.total_equity),
        )
    return item


@app.get("/latest/{company_id}", response_model=models.BalanceSheet)
async def get_latest(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.get_latest(db_session, user_id, company_id)
    if item is None:
        raise NotFoundError(detail="No balance sheets found")
    return item


@app.get("/history/{company_id}")
async def get_history(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    sheets = await crud.get_history(db_session, user_id, company_id)
    return {"company_id": company_id, "sheets": sheets, "total": len(sheets)}


@app.get("/ratios/{company_id}")
async def get_ratios(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.get_latest(db_session, user_id, company_id)
    if item is None:
        raise NotFoundError(detail="No balance sheets found")
    current_assets = sum(a.amount for a in item.assets if a.category == "current")
    current_liabilities = sum(l.amount for l in item.liabilities if l.category == "current")
    liquid_assets = sum(a.amount for a in item.assets if a.is_liquid)
    return {
        "current_ratio": current_assets / max(1, current_liabilities),
        "quick_ratio": liquid_assets / max(1, current_liabilities),
        "debt_to_equity": item.total_liabilities / max(1, item.total_equity),
        "debt_to_assets": item.total_liabilities / max(1, item.total_assets),
        "equity_ratio": item.total_equity / max(1, item.total_assets),
    }


@app.exception_handler(NotFoundError)
async def not_found_handler(request: Request, exc: NotFoundError):
    from fastapi.responses import JSONResponse

    return JSONResponse(status_code=404, content={"detail": exc.detail})


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
