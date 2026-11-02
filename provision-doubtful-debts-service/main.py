"""
Vimbai Provision for Doubtful Debts Service
Debtors under review and the provisions raised from them. Port: 8035

Persistence is Neo4j, scoped to the caller and the Book. The file may be imported
bare (bracket mounts, uvicorn main:app), so it bootstraps its own package alias
first.

FIXED LATENT BUGS, both in the original in-memory version:
* `/provision/create` built its record from the summary dicts `/calculate-provision`
  returns into a field typed List[DoubtfulDebtEntry], so every call raised a
  pydantic ValidationError and answered 500. The record now carries the full
  entries.
* the journal it sent to the accounting service embedded datetime objects and was
  never JSON serialisable, so the reference silently came back empty. The payload
  is now serialised, and a failed post still leaves the reference unset.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "provision_doubtful_debts_service" not in _sys.modules or not hasattr(
    _sys.modules.get("provision_doubtful_debts_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "provision_doubtful_debts_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["provision_doubtful_debts_service"] = _pkg
    _sys.modules["provision_doubtful_debts_service"].__path__ = [_HERE]

import os
from datetime import datetime
from typing import Any, Dict, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, HTTPException, Request, status
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from provision_doubtful_debts_service import crud
from provision_doubtful_debts_service.dependencies import book_id_var, get_db_session, get_user_id
from provision_doubtful_debts_service.exceptions import NotFoundError
from provision_doubtful_debts_service.models import ProvisionType

SERVICE_NAME = "provision-doubtful-debts-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8034"))
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

app = FastAPI(title="Vimbai Provision for Doubtful Debts Service", version=SERVICE_VERSION, docs_url="/docs")
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
    except Exception as e:
        logger.error("error", error=str(e))
        return {}


async def call_audit_service(action: str, resource_type: str, resource_id: str, details: Dict[str, Any]):
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            await client.post(
                f"{AUDIT_SERVICE_URL}/audit",
                json={
                    "action": action,
                    "resource_type": resource_type,
                    "resource_id": resource_id,
                    "details": details,
                    "timestamp": datetime.utcnow().isoformat(),
                },
            )
    except Exception:
        pass


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "description": "Provision for doubtful debts management",
    }


@app.post("/debtors/add")
async def add_debtor(
    debtor_id: str,
    debtor_name: str,
    invoice_number: str,
    invoice_date: datetime,
    due_date: datetime,
    amount: float,
    risk_assessment: str = "medium",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add a debtor for doubtful debt provision tracking."""
    entry = await crud.add_debtor(
        db_session,
        user_id,
        debtor_id=debtor_id,
        debtor_name=debtor_name,
        invoice_number=invoice_number,
        invoice_date=invoice_date,
        due_date=due_date,
        amount=amount,
        risk_assessment=risk_assessment,
    )
    await call_audit_service("CREATE", "debtor", debtor_id, {"name": debtor_name})
    return entry


@app.post("/debtors/{debtor_id}/assess")
async def assess_debtor_risk(
    debtor_id: str,
    risk_assessment: str,
    notes: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update debtor risk assessment."""
    try:
        entry = await crud.assess_debtor(db_session, user_id, debtor_id, risk_assessment, notes)
    except NotFoundError:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Debtor not found")
    await call_audit_service("ASSESS", "debtor", debtor_id, {"risk": risk_assessment})
    return entry


@app.post("/calculate-provision")
async def calculate_provision(
    period_end: datetime,
    provision_type: ProvisionType = ProvisionType.SPECIFIC,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate total provision required."""
    return await crud.calculate_provision(db_session, user_id, period_end, provision_type)


@app.post("/provision/create")
async def create_provision(
    period_end: datetime,
    provision_opening: float = 0,
    provision_type: ProvisionType = ProvisionType.SPECIFIC,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create provision journal entry."""
    calc = await crud.calculate_provision(db_session, user_id, period_end, provision_type)
    provision_required = calc["total_provision_required"]
    provision_adjustment = provision_required - provision_opening

    journal_entry = {
        "date": period_end.isoformat(),
        "description": f"Provision for doubtful debts - {period_end.date()}",
        "entries": [
            {
                "account_code": "6300",
                "description": "Bad Debts Expense",
                "debit": abs(provision_adjustment) if provision_adjustment > 0 else 0,
                "credit": 0,
            },
            {
                "account_code": "1310",
                "description": "Provision for Doubtful Debts",
                "debit": 0,
                "credit": abs(provision_adjustment) if provision_adjustment > 0 else provision_adjustment,
            },
        ],
        "reference": f"DDBT-PROV-{period_end.strftime('%Y%m')}",
    }

    result = await call_accounting_service("POST", "/journal-entries", journal_entry)

    # the full debtor entries, not the summary dicts the calculation reports
    entries = await crud.list_debtors(db_session, user_id)
    provision = await crud.create_provision(
        db_session,
        user_id,
        period_end,
        provision_opening=provision_opening,
        provision_type=provision_type,
        entries=entries,
        journal_entry_id=result.get("id"),
    )

    await call_audit_service("CREATE", "provision", provision.id, {"amount": provision_required})
    return provision


@app.get("/provisions")
async def list_provisions(
    limit: int = 12,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get provision history."""
    return {"provisions": await crud.list_provisions(db_session, user_id, limit)}


@app.get("/debtors")
async def list_debtors(
    risk_assessment: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all debtors."""
    debtors = await crud.list_debtors(db_session, user_id, risk_assessment)
    return {"debtors": debtors, "count": len(debtors)}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
