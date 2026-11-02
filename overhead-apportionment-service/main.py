"""
Vimbai Overhead Apportionment Service
Handles overhead apportionment to production cost centres.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "overhead_apportionment_service" not in _sys.modules or not hasattr(
    _sys.modules.get("overhead_apportionment_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "overhead_apportionment_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["overhead_apportionment_service"] = _pkg
    _sys.modules["overhead_apportionment_service"].__path__ = [_HERE]

import os
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from overhead_apportionment_service import calc, crud
from overhead_apportionment_service.dependencies import book_id_var, get_db_session, get_user_id
from overhead_apportionment_service.models import ApportionedOverhead, OverheadItem

SERVICE_NAME = "overhead-apportionment-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8071"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

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

app = FastAPI(title="Vimbai Overhead Apportionment Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


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
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "status": "healthy",
        "calculation_engine": calc.engine_in_use(),
    }


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Overhead apportionment"}


@app.post("/overheads/add")
async def add_overhead_item(
    overhead_name: str,
    overhead_code: str,
    total_amount: float,
    basis: str,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Add an overhead item."""
    overhead = OverheadItem(
        overhead_name=overhead_name, overhead_code=overhead_code, total_amount=total_amount, basis=basis
    )
    return await crud.create(session, user_id, overhead)


async def _apportion_stored_overhead(
    session: AsyncSession, user_id: str, overhead: OverheadItem, period: str, cost_centre_data
):
    result = calc.apportion(
        {
            "overhead_id": overhead.id,
            "overhead_name": overhead.overhead_name,
            "total_amount": overhead.total_amount,
            "basis": overhead.basis,
        },
        period,
        cost_centre_data,
    )
    stored = ApportionedOverhead(
        overhead_id=result["overhead_id"],
        period=result["period"],
        cost_centre_apportionments=result["cost_centre_apportionments"],
        total_overhead=result["total_overhead"],
    )
    return await crud.create(session, user_id, stored)


@app.post("/apportion")
async def apportion_overhead(
    overhead_id: str,
    period: str,
    cost_centre_data: List[Dict[str, Any]],
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Apportion overhead to cost centres."""
    overhead = await crud.find(session, user_id, OverheadItem, overhead_id)
    if not overhead:
        return {"error": "Overhead not found"}
    return await _apportion_stored_overhead(session, user_id, overhead, period, cost_centre_data)


@app.post("/batch-apportion")
async def batch_apportion_overheads(
    period: str,
    cost_centre_data: List[Dict[str, Any]],
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Apportion all overheads for a period."""
    results = []
    for overhead in await crud.list_all(session, user_id, OverheadItem):
        results.append(await _apportion_stored_overhead(session, user_id, overhead, period, cost_centre_data))
    return {"period": period, "apportionment_results": results}


@app.get("/overheads")
async def list_overhead_items(
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List overhead items."""
    return {"overheads": await crud.list_all(session, user_id, OverheadItem)}


@app.get("/apportionments")
async def list_apportionments(
    overhead_id: Optional[str] = None,
    period: Optional[str] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List apportionment results."""
    result = await crud.list_all(session, user_id, ApportionedOverhead)
    if overhead_id:
        result = [a for a in result if a.overhead_id == overhead_id]
    if period:
        result = [a for a in result if a.period == period]
    return {"apportionments": result}


@app.get("/cost-centre-summary/{period}")
async def get_cost_centre_overhead_summary(
    period: str,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Get total overhead apportioned to each cost centre.

    Reads the caller's stored apportionments. The original scanned the unused
    ``apportionment_overheads`` list, so the summary always came back empty;
    that behaviour is pinned as a regression case in the service tests.
    """
    centre_totals = {}

    for apportionment in await crud.list_all(session, user_id, ApportionedOverhead):
        if apportionment.period != period:
            continue
        for centre in apportionment.cost_centre_apportionments:
            centre_id = centre["cost_centre_id"]
            if centre_id not in centre_totals:
                centre_totals[centre_id] = {
                    "cost_centre_id": centre_id,
                    "cost_centre_name": centre["cost_centre_name"],
                    "total_overhead": 0,
                    "breakdown": [],
                }
            centre_totals[centre_id]["total_overhead"] += centre["apportioned_amount"]
            centre_totals[centre_id]["breakdown"].append(
                {
                    "overhead_id": apportionment.overhead_id,
                    "apportioned_amount": centre["apportioned_amount"],
                    "basis": centre["basis"],
                }
            )

    return {"period": period, "cost_centre_summary": list(centre_totals.values())}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
