"""
Vimbai Bad Debts Provision Service
Calculates and journals the provision for doubtful debts. Port: 8034

Policies, debtors and provision journals live in Neo4j, scoped to the caller and
the Book. The file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "provision_bad_debts_service" not in _sys.modules or not hasattr(
    _sys.modules.get("provision_bad_debts_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("provision_bad_debts_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["provision_bad_debts_service"] = _pkg
    _sys.modules["provision_bad_debts_service"].__path__ = [_HERE]

import os
from datetime import datetime
from typing import Any, Dict, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, Request, status
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from provision_bad_debts_service import crud
from provision_bad_debts_service.dependencies import book_id_var, get_db_session, get_user_id
from provision_bad_debts_service.models import ProvisionEntry, ProvisionJournal, ProvisionPolicy

SERVICE_NAME = "provision-bad-debts-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8032"))
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

app = FastAPI(title="Vimbai Provision for Bad Debts Service", version=SERVICE_VERSION, docs_url="/docs")
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
        logger.error("accounting_service_error", error=str(e))
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
    except Exception as e:
        logger.error("audit_error", error=str(e))


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "description": "Provision for bad and doubtful debts management",
    }


@app.post("/policies", response_model=ProvisionPolicy, status_code=status.HTTP_201_CREATED)
async def create_policy(
    data: ProvisionPolicy,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a provision policy."""
    policy = await crud.create_policy(db_session, user_id, data)
    await call_audit_service("CREATE", "provision_policy", policy.id, {"name": policy.name, "method": policy.method})
    return policy


@app.get("/policies")
async def list_policies(
    is_active: Optional[bool] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List provision policies."""
    return {"policies": await crud.list_policies(db_session, user_id, is_active)}


@app.post("/debtors/add")
async def add_debtor(
    debtor_id: str,
    debtor_name: str,
    invoice_number: str,
    original_amount: float,
    invoice_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add a debtor for tracking."""
    entry = await crud.add_debtor(
        db_session,
        user_id,
        debtor_id=debtor_id,
        debtor_name=debtor_name,
        invoice_number=invoice_number,
        original_amount=original_amount,
    )
    await call_audit_service("CREATE", "debtor", debtor_id, {"name": debtor_name})
    return entry


@app.post("/calculate-provision")
async def calculate_provision(
    period_end: datetime,
    policy_id: Optional[str] = None,
    total_debtors: float = 0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate provision for bad debts."""
    return await crud.calculate_provision(
        db_session, user_id, period_end, policy_id=policy_id, total_debtors=total_debtors
    )


@app.post("/journal/create")
async def create_provision_journal(
    period_end: datetime,
    total_debtors: float,
    provision_opening: float = 0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create journal entry for provision."""
    # the original never threads a policy through here, so this always ages
    # against the default ladder
    calc = await crud.calculate_provision(db_session, user_id, period_end, total_debtors=total_debtors)
    provision_required = calc["provision_required"]
    provision_adjustment = provision_required - provision_opening

    journal_entry = {
        "date": period_end,
        "description": f"Provision for doubtful debts as at {period_end.date()}",
        "entries": [
            {
                "account_code": "6300",
                "description": "Bad Debts Expense",
                "debit": provision_adjustment if provision_adjustment > 0 else 0,
                "credit": 0,
            },
            {
                "account_code": "1310",
                "description": "Provision for Doubtful Debts",
                "debit": 0,
                "credit": provision_adjustment if provision_adjustment > 0 else abs(provision_adjustment),
            },
        ],
        "reference": f"PROV-{period_end.strftime('%Y%m')}",
    }

    result = await call_accounting_service("POST", "/journal-entries", journal_entry)

    journal = await crud.create_journal(
        db_session,
        user_id,
        period_end,
        total_debtors,
        provision_opening=provision_opening,
        provision_required=provision_required,
        entries=calc["entries"],
        journal_entry_id=result.get("id"),
    )

    await call_audit_service("CREATE", "provision_journal", journal.id, {"amount": provision_required})
    return journal


@app.get("/provision-history")
async def get_provision_history(
    limit: int = 12,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get provision history."""
    return {"journals": await crud.list_journals(db_session, user_id, limit)}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
