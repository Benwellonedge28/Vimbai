"""
Vimbai Partnership Revaluation Service
Handles asset revaluations during partnership changes.

Revaluation reports persist in Neo4j, stamped with the caller
(X-User-Id) and the Book context (X-Book-ID, verified upstream by the
API gateway). Listings and lookups are scoped to the caller's own
Book-visible records. Realization math, list shape, and the
{"error": "Revaluation not found"} miss response are preserved.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
from datetime import datetime
from typing import Any, Dict, List, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "partnership_revaluation_service" not in _sys.modules or not hasattr(
    _sys.modules.get("partnership_revaluation_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "partnership_revaluation_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["partnership_revaluation_service"] = _pkg
    _sys.modules["partnership_revaluation_service"].__path__ = [_HERE]

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from partnership_revaluation_service import crud
from partnership_revaluation_service.database import Neo4jConnector
from partnership_revaluation_service.dependencies import book_id_var, get_db_session, get_user_id
from partnership_revaluation_service.exceptions import PartnershipRevaluationServiceError
from partnership_revaluation_service.models import GoodwillTreatment, RevaluationEntry, RevaluationReport

SERVICE_NAME = "partnership-revaluation-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8044"))
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

app = FastAPI(title="Vimbai Partnership Revaluation Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(PartnershipRevaluationServiceError)
async def _pr_error(request: Request, exc: PartnershipRevaluationServiceError):
    from fastapi.responses import JSONResponse

    return JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
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
    return {"service": SERVICE_NAME, "description": "Partnership asset revaluation service"}


@app.post("/revalue")
async def create_revaluation(
    partnership_id: str,
    revaluation_date: datetime,
    goodwill_treatment: GoodwillTreatment,
    asset_revaluations: List[Dict[str, Any]],
    goodwill_amount: float = 0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create asset revaluation."""
    entries = []
    total_increase = 0
    total_decrease = 0
    journal_entries = []

    for rev in asset_revaluations:
        entry = RevaluationEntry(
            asset_id=rev["asset_id"],
            asset_name=rev["asset_name"],
            asset_code=rev["asset_code"],
            old_value=rev["old_value"],
            new_value=rev["new_value"],
        )
        if entry.new_value > entry.old_value:
            entry.increase = entry.new_value - entry.old_value
            entry.revaluation_gain = entry.increase
            total_increase += entry.increase
            journal_entries.append(
                {
                    "date": revaluation_date,
                    "description": f"Revaluation gain - {entry.asset_name}",
                    "entries": [
                        {
                            "account_code": rev["account_code"],
                            "description": entry.asset_name,
                            "debit": entry.increase,
                            "credit": 0,
                        },
                        {
                            "account_code": "3100",
                            "description": "Revaluation Reserve",
                            "debit": 0,
                            "credit": entry.increase,
                        },
                    ],
                    "reference": f"REV-{entry.id[:8]}",
                }
            )
        else:
            entry.decrease = entry.old_value - entry.new_value
            entry.revaluation_loss = entry.decrease
            total_decrease += entry.decrease
            journal_entries.append(
                {
                    "date": revaluation_date,
                    "description": f"Revaluation loss - {entry.asset_name}",
                    "entries": [
                        {
                            "account_code": "6200",
                            "description": "Revaluation Loss",
                            "debit": entry.decrease,
                            "credit": 0,
                        },
                        {
                            "account_code": rev["account_code"],
                            "description": entry.asset_name,
                            "debit": 0,
                            "credit": entry.decrease,
                        },
                    ],
                    "reference": f"REV-{entry.id[:8]}",
                }
            )
        entries.append(entry)

    # Goodwill treatment
    if goodwill_amount > 0 and goodwill_treatment == GoodwillTreatment.RAISE_AND_RAISE:
        journal_entries.append(
            {
                "date": revaluation_date,
                "description": "Goodwill arising on revaluation",
                "entries": [
                    {"account_code": "1500", "description": "Goodwill", "debit": goodwill_amount, "credit": 0},
                    {
                        "account_code": "3100",
                        "description": "Revaluation Reserve",
                        "debit": 0,
                        "credit": goodwill_amount,
                    },
                ],
                "reference": f"GW-REV-{revaluation_date.strftime('%Y%m')}",
            }
        )

    # Post all journal entries (fail-soft: accounting service may be absent in tests)
    entry_ids = []
    for entry in journal_entries:
        result = await call_accounting_service("POST", "/journal-entries", entry)
        entry_ids.append(result.get("id", ""))
        for reventry in entries:
            if entry["reference"].endswith(reventry.id[:8]):
                reventry.journal_entry_id = result.get("id")

    report = RevaluationReport(
        partnership_id=partnership_id,
        revaluation_date=revaluation_date,
        entries=entries,
        total_increase=total_increase,
        total_decrease=total_decrease,
        net_gain=total_increase - total_decrease,
        goodwill_amount=goodwill_amount,
        goodwill_treatment=goodwill_treatment,
        journal_entry_ids=entry_ids,
    )
    return await crud.create_revaluation(db_session, user_id, report)


@app.get("/revaluations")
async def list_revaluations(
    partnership_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's revaluations."""
    result = await crud.list_revaluations(db_session, user_id)
    if partnership_id:
        result = [r for r in result if r.partnership_id == partnership_id]
    return {"revaluations": result, "count": len(result)}


@app.get("/revaluations/{revaluation_id}")
async def get_revaluation(
    revaluation_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get revaluation details (caller-scoped)."""
    rev = await crud.get_revaluation(db_session, user_id, revaluation_id)
    if not rev:
        return {"error": "Revaluation not found"}
    return rev


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
