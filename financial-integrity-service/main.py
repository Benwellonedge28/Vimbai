"""Vimbai Financial Integrity Service - balance, hash and completeness checks. Port: 8332

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "financial_integrity_service" not in _sys.modules or not hasattr(
    _sys.modules.get("financial_integrity_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("financial_integrity_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["financial_integrity_service"] = _pkg
    _sys.modules["financial_integrity_service"].__path__ = [_HERE]

import hashlib
import os

import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from financial_integrity_service import crud, models
from financial_integrity_service.dependencies import book_id_var, get_db_session, get_user_id
from neo4j import AsyncSession

SERVICE_NAME = "financial-integrity-service"
PORT = int(os.getenv("PORT", "8332"))
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
app = FastAPI(title="Vimbai Financial Integrity Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
# Distributed tracing (OpenTelemetry)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="financial-integrity-service", instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/check/balance")
async def check_balance(
    company_id: str,
    account_id: str,
    debits: float,
    credits: float,
    tolerance: float = 0.01,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Balance check (pure computation); the result is recorded for the caller's Book."""
    passed = abs(debits - credits) <= tolerance
    check = models.IntegrityCheck(
        company_id=company_id,
        check_type="balance_check",
        entity_type="account",
        entity_id=account_id,
        passed=passed,
        details=f"Debits: {debits}, Credits: {credits}, Diff: {abs(debits-credits)}",
    )
    await crud.record_check(db_session, user_id, check)
    return {"passed": passed, "difference": abs(debits - credits), "tolerance": tolerance}


@app.post("/check/hash")
async def verify_hash(
    company_id: str,
    entity_type: str,
    entity_id: str,
    data: str,
    expected_hash: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """SHA-256 hash verification; the result is recorded for the caller's Book."""
    actual_hash = hashlib.sha256(data.encode()).hexdigest()
    passed = actual_hash == expected_hash
    check = models.IntegrityCheck(
        company_id=company_id,
        check_type="hash_verify",
        entity_type=entity_type,
        entity_id=entity_id,
        passed=passed,
        hash_before=expected_hash,
        hash_after=actual_hash,
        details="Hash mismatch" if not passed else "Hash verified",
    )
    await crud.record_check(db_session, user_id, check)
    return {"passed": passed, "actual_hash": actual_hash, "expected_hash": expected_hash}


@app.post("/check/completeness")
async def check_completeness(
    company_id: str,
    entity_type: str,
    expected_count: int,
    actual_count: int,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Count completeness check; the result is recorded for the caller's Book."""
    passed = expected_count == actual_count
    check = models.IntegrityCheck(
        company_id=company_id,
        check_type="completeness",
        entity_type=entity_type,
        passed=passed,
        details=f"Expected: {expected_count}, Actual: {actual_count}",
    )
    await crud.record_check(db_session, user_id, check)
    return {
        "passed": passed,
        "expected": expected_count,
        "actual": actual_count,
        "missing": expected_count - actual_count,
    }


@app.get("/report/{company_id}", response_model=models.IntegrityReport)
async def get_report(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Integrity report over the caller's Book-visible checks for the company."""
    checks = await crud.list_checks(db_session, user_id, company_id)
    passed = sum(1 for c in checks if c.passed)
    failed = len(checks) - passed
    return models.IntegrityReport(
        company_id=company_id,
        total_checks=len(checks),
        passed=passed,
        failed=failed,
        pass_rate=passed / max(1, len(checks)) * 100,
        checks=checks,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
