"""
Vimbai General Reserve Service
Handles general reserves: creation, allocation, and utilization.

Records persist in Neo4j, stamped with the caller (X-User-Id) and the
Book context (X-Book-ID, verified upstream by the API gateway).
Allocate/utilize check the caller's own Book-visible reserve first.
Balance math, journal side-calls, miss contracts, and response shapes
are preserved exactly.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
from datetime import datetime, timezone
from typing import Any, Dict, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "general_reserve_service" not in _sys.modules or not hasattr(
    _sys.modules.get("general_reserve_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("general_reserve_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["general_reserve_service"] = _pkg
    _sys.modules["general_reserve_service"].__path__ = [_HERE]

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from general_reserve_service import crud
from general_reserve_service.database import Neo4jConnector
from general_reserve_service.dependencies import book_id_var, get_db_session, get_user_id
from general_reserve_service.exceptions import GeneralReserveServiceError
from general_reserve_service.models import GeneralReserve, ReserveAllocation, ReserveUtilization
from neo4j import AsyncSession

SERVICE_NAME = "general-reserve-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8064"))
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

app = FastAPI(title="Vimbai General Reserve Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(GeneralReserveServiceError)
async def _gr_error(request: Request, exc: GeneralReserveServiceError):
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "General reserve management"}


@app.post("/reserves/create")
async def create_reserve(
    company_id: str,
    reserve_name: str,
    description: str = "",
    initial_balance: float = 0,
    target_balance: Optional[float] = None,
    minimum_balance: float = 0,
    funding_source: str = "retained_earnings",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a general reserve."""
    reserve = GeneralReserve(
        company_id=company_id,
        reserve_name=reserve_name,
        description=description,
        current_balance=initial_balance,
        target_balance=target_balance,
        minimum_balance=minimum_balance,
        funding_source=funding_source,
    )

    if initial_balance > 0:
        journal_entry = {
            "date": datetime.utcnow(),
            "description": f"Creation of {reserve_name} reserve",
            "entries": [
                {"account_code": "3300", "description": "Retained Earnings", "debit": initial_balance, "credit": 0},
                {"account_code": "3310", "description": "General Reserve", "debit": 0, "credit": initial_balance},
            ],
            "reference": f"RESERVE-CREATE-{reserve.id[:8]}",
        }
        result = await call_accounting_service("POST", "/journal-entries", journal_entry)
        reserve.journal_entry_id = result.get("id")

    return await crud.create_reserve(db_session, user_id, reserve)


@app.post("/reserves/{reserve_id}/allocate")
async def allocate_to_reserve(
    reserve_id: str,
    amount: float,
    source: str,
    description: str,
    allocation_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Allocate funds to reserve."""
    reserve = await crud.get_reserve(db_session, user_id, reserve_id)
    if not reserve:
        return {"error": "Reserve not found"}

    if allocation_date is None:
        allocation_date = datetime.now(timezone.utc)

    allocation = ReserveAllocation(
        reserve_id=reserve_id, amount=amount, allocation_date=allocation_date, source=source, description=description
    )

    reserve.current_balance += amount
    reserve.updated_at = datetime.now(timezone.utc)
    await crud.save_reserve(db_session, user_id, reserve)

    source_account = "3300" if source == "retained_earnings" else "3210"
    journal_entry = {
        "date": allocation_date,
        "description": f"Allocation to {reserve.reserve_name}: {description}",
        "entries": [
            {
                "account_code": source_account,
                "description": source.replace("_", " ").title(),
                "debit": amount,
                "credit": 0,
            },
            {"account_code": "3310", "description": "General Reserve", "debit": 0, "credit": amount},
        ],
        "reference": f"RESERVE-ALLOC-{allocation.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    allocation.journal_entry_id = result.get("id")
    allocation = await crud.create_allocation(db_session, user_id, allocation)

    return {"reserve": reserve, "allocation": allocation}


@app.post("/reserves/{reserve_id}/utilize")
async def utilize_reserve(
    reserve_id: str,
    amount: float,
    purpose: str,
    description: str,
    utilization_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Utilize funds from reserve."""
    reserve = await crud.get_reserve(db_session, user_id, reserve_id)
    if not reserve:
        return {"error": "Reserve not found"}

    if amount > reserve.current_balance:
        return {"error": "Insufficient reserve balance"}

    if utilization_date is None:
        utilization_date = datetime.now(timezone.utc)

    utilization = ReserveUtilization(
        reserve_id=reserve_id,
        amount=amount,
        utilization_date=utilization_date,
        purpose=purpose,
        description=description,
    )

    reserve.current_balance -= amount
    reserve.updated_at = datetime.now(timezone.utc)
    await crud.save_reserve(db_session, user_id, reserve)

    dest_account = "1000" if purpose in ["asset_purchase", "working_capital"] else "2310"
    journal_entry = {
        "date": utilization_date,
        "description": f"Utilization of {reserve.reserve_name}: {description}",
        "entries": [
            {"account_code": "3310", "description": "General Reserve", "debit": amount, "credit": 0},
            {
                "account_code": dest_account,
                "description": purpose.replace("_", " ").title(),
                "debit": 0,
                "credit": amount,
            },
        ],
        "reference": f"RESERVE-UTIL-{utilization.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    utilization.journal_entry_id = result.get("id")
    utilization = await crud.create_utilization(db_session, user_id, utilization)

    return {"reserve": reserve, "utilization": utilization}


@app.get("/reserves")
async def list_reserves(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all reserves."""
    result = await crud.list_reserves(db_session, user_id)
    if company_id:
        result = [r for r in result if r.company_id == company_id]
    return {"reserves": result}


@app.get("/reserves/{reserve_id}")
async def get_reserve(
    reserve_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get reserve details."""
    reserve = await crud.get_reserve(db_session, user_id, reserve_id)
    if not reserve:
        return {"error": "Reserve not found"}
    return reserve


@app.get("/reserves/{reserve_id}/history")
async def get_reserve_history(
    reserve_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get reserve allocation and utilization history."""
    reserve_allocations = await crud.list_allocations(db_session, user_id, reserve_id)
    reserve_utilizations = await crud.list_utilizations(db_session, user_id, reserve_id)
    return {"allocations": reserve_allocations, "utilizations": reserve_utilizations}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
