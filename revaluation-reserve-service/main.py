"""
Vimbai Revaluation Reserve Service
Handles revaluation reserve: asset revaluations and utilization.

Records persist in Neo4j, stamped with the caller (X-User-Id) and the
Book context (X-Book-ID, verified upstream by the API gateway).
Cumulative revaluation nodes are per caller+asset (upsert). Gain/loss
math, journal side-calls, miss contracts, and response shapes are
preserved exactly.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
from datetime import datetime, timezone
from typing import Any, Dict, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "revaluation_reserve_service" not in _sys.modules or not hasattr(
    _sys.modules.get("revaluation_reserve_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("revaluation_reserve_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["revaluation_reserve_service"] = _pkg
    _sys.modules["revaluation_reserve_service"].__path__ = [_HERE]

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from revaluation_reserve_service import crud
from revaluation_reserve_service.database import Neo4jConnector
from revaluation_reserve_service.dependencies import book_id_var, get_db_session, get_user_id
from revaluation_reserve_service.exceptions import RevaluationReserveServiceError
from revaluation_reserve_service.models import CumulativeRevaluation, RevaluationEntry, RevaluationUtilization

SERVICE_NAME = "revaluation-reserve-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8065"))
ACCOUNTING_SERVICE_URL = _os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Revaluation Reserve Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(RevaluationReserveServiceError)
async def _rr_error(request: Request, exc: RevaluationReserveServiceError):
    from fastapi.responses import JSONResponse

    return JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "POST":
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception:
        return {}


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Revaluation reserve management"}


@app.post("/revaluations/record")
async def record_revaluation(
    company_id: str,
    asset_id: str,
    asset_name: str,
    asset_class: str,
    revaluation_date: datetime,
    previous_value: float,
    new_value: float,
    depreciation_adjustment: float = 0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record asset revaluation."""
    revaluation_gain = 0
    revaluation_loss = 0

    if new_value > previous_value:
        revaluation_gain = new_value - previous_value
    else:
        revaluation_loss = previous_value - new_value

    # Adjust for depreciation impact
    net_effect = revaluation_gain - revaluation_loss - depreciation_adjustment

    entry = RevaluationEntry(
        company_id=company_id,
        asset_id=asset_id,
        asset_name=asset_name,
        asset_class=asset_class,
        revaluation_date=revaluation_date,
        previous_value=previous_value,
        new_value=new_value,
        revaluation_gain=revaluation_gain,
        revaluation_loss=revaluation_loss,
        depreciation_adjustment=depreciation_adjustment,
        net_effect=net_effect,
    )

    asset_account = "1500" if asset_class == "property" else "1600"

    if revaluation_gain > 0:
        journal_entry = {
            "date": revaluation_date,
            "description": f"Revaluation of {asset_name}",
            "entries": [
                {"account_code": asset_account, "description": asset_name, "debit": revaluation_gain, "credit": 0},
                {"account_code": "3320", "description": "Revaluation Reserve", "debit": 0, "credit": revaluation_gain},
            ],
            "reference": f"REV-{entry.id[:8]}",
        }
    else:
        journal_entry = {
            "date": revaluation_date,
            "description": f"Revaluation of {asset_name} (downward)",
            "entries": [
                {"account_code": "3320", "description": "Revaluation Reserve", "debit": revaluation_loss, "credit": 0},
                {"account_code": asset_account, "description": asset_name, "debit": 0, "credit": revaluation_loss},
            ],
            "reference": f"REV-{entry.id[:8]}",
        }

    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    entry.journal_entry_id = result.get("id")
    entry = await crud.create_entry(db_session, user_id, entry)

    # Update cumulative revaluation (caller's own node for the asset)
    cumulative = await crud.get_cumulative(db_session, user_id, asset_id)
    if cumulative:
        cumulative.total_revaluation_gain += revaluation_gain
        cumulative.total_revaluation_loss += revaluation_loss
        cumulative.net_revaluation_reserve = (
            cumulative.total_revaluation_gain - cumulative.total_revaluation_loss - cumulative.total_utilized
        )
        cumulative.last_revaluation_date = revaluation_date
    else:
        cumulative = CumulativeRevaluation(
            company_id=company_id,
            asset_id=asset_id,
            total_revaluation_gain=revaluation_gain,
            total_revaluation_loss=revaluation_loss,
            net_revaluation_reserve=revaluation_gain - revaluation_loss,
            last_revaluation_date=revaluation_date,
        )

    cumulative = await crud.save_cumulative(db_session, user_id, cumulative)
    return {"revaluation": entry, "cumulative": cumulative}


@app.post("/utilizations/record")
async def utilize_revaluation_reserve(
    company_id: str,
    amount: float,
    utilization_type: str,
    related_asset_id: Optional[str] = None,
    description: str = "",
    utilization_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Utilize revaluation reserve."""
    if utilization_date is None:
        utilization_date = datetime.now(timezone.utc)

    utilization = RevaluationUtilization(
        company_id=company_id,
        amount=amount,
        utilization_type=utilization_type,
        related_asset_id=related_asset_id,
        description=description,
        utilization_date=utilization_date,
    )

    if utilization_type == "asset_disposal":
        journal_entry = {
            "date": utilization_date,
            "description": f"Revaluation reserve on asset disposal: {description}",
            "entries": [
                {"account_code": "3320", "description": "Revaluation Reserve", "debit": amount, "credit": 0},
                {"account_code": "3300", "description": "Retained Earnings", "debit": 0, "credit": amount},
            ],
            "reference": f"REV-UTIL-{utilization.id[:8]}",
        }
    elif utilization_type == "impairment":
        journal_entry = {
            "date": utilization_date,
            "description": f"Revaluation reserve impairment: {description}",
            "entries": [
                {"account_code": "3320", "description": "Revaluation Reserve", "debit": amount, "credit": 0},
                {"account_code": "1650", "description": "Impairment Loss", "debit": 0, "credit": amount},
            ],
            "reference": f"REV-UTIL-{utilization.id[:8]}",
        }
    else:  # transfer_retained_earnings
        journal_entry = {
            "date": utilization_date,
            "description": f"Transfer to retained earnings: {description}",
            "entries": [
                {"account_code": "3320", "description": "Revaluation Reserve", "debit": amount, "credit": 0},
                {"account_code": "3300", "description": "Retained Earnings", "debit": 0, "credit": amount},
            ],
            "reference": f"REV-UTIL-{utilization.id[:8]}",
        }

    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    utilization.journal_entry_id = result.get("id")

    return await crud.create_utilization(db_session, user_id, utilization)


@app.get("/revaluations")
async def list_revaluations(
    company_id: Optional[str] = None,
    asset_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List revaluation entries."""
    result = await crud.list_entries(db_session, user_id)
    if company_id:
        result = [r for r in result if r.company_id == company_id]
    if asset_id:
        result = [r for r in result if r.asset_id == asset_id]
    return {"revaluations": result}


@app.get("/cumulative/{asset_id}")
async def get_cumulative_revaluation(
    asset_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get cumulative revaluation for an asset."""
    cumulative = await crud.get_cumulative(db_session, user_id, asset_id)
    if not cumulative:
        return {"error": "Asset not found"}
    return cumulative


@app.get("/summary/{company_id}")
async def get_revaluation_summary(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get revaluation reserve summary."""
    company_revaluations = [r for r in await crud.list_entries(db_session, user_id) if r.company_id == company_id]
    company_utilizations = [u for u in await crud.list_utilizations(db_session, user_id) if u.company_id == company_id]

    total_gains = sum(r.revaluation_gain for r in company_revaluations)
    total_losses = sum(r.revaluation_loss for r in company_revaluations)
    total_utilized = sum(u.amount for u in company_utilizations)

    return {
        "company_id": company_id,
        "total_revaluation_gains": total_gains,
        "total_revaluation_losses": total_losses,
        "total_utilized": total_utilized,
        "net_revaluation_reserve": total_gains - total_losses - total_utilized,
        "assets_revalued": len(set(r.asset_id for r in company_revaluations)),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
