"""Vimbai Financial Integrity Service - Financial data integrity checks. Port: 8373"""
import os, uuid, hashlib
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "financial-integrity-service"
PORT = int(os.getenv("PORT", "8373"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Financial Integrity Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="financial-integrity-service", instrument_app=app)
except ImportError:
    TRACER = None

class IntegrityCheck(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    check_type: str  # balance_check, hash_verify, reconciliation, completeness
    entity_type: str = ""
    entity_id: str = ""
    passed: bool = False
    details: str = ""
    hash_before: str = ""
    hash_after: str = ""
    checked_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))

class IntegrityReport(BaseModel):
    company_id: str
    total_checks: int = 0
    passed: int = 0
    failed: int = 0
    pass_rate: float = 0
    checks: List[IntegrityCheck] = []

_checks: Dict[str, List[IntegrityCheck]] = defaultdict(list)

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/check/balance")
async def check_balance(company_id: str, account_id: str, debits: float, credits: float, tolerance: float = 0.01):
    passed = abs(debits - credits) <= tolerance
    check = IntegrityCheck(company_id=company_id, check_type="balance_check", entity_type="account", entity_id=account_id, passed=passed, details=f"Debits: {debits}, Credits: {credits}, Diff: {abs(debits-credits)}")
    _checks[company_id].append(check)
    return {"passed": passed, "difference": abs(debits - credits), "tolerance": tolerance}

@app.post("/check/hash")
async def verify_hash(company_id: str, entity_type: str, entity_id: str, data: str, expected_hash: str):
    actual_hash = hashlib.sha256(data.encode()).hexdigest()
    passed = actual_hash == expected_hash
    check = IntegrityCheck(company_id=company_id, check_type="hash_verify", entity_type=entity_type, entity_id=entity_id, passed=passed, hash_before=expected_hash, hash_after=actual_hash, details="Hash mismatch" if not passed else "Hash verified")
    _checks[company_id].append(check)
    return {"passed": passed, "actual_hash": actual_hash, "expected_hash": expected_hash}

@app.post("/check/completeness")
async def check_completeness(company_id: str, entity_type: str, expected_count: int, actual_count: int):
    passed = expected_count == actual_count
    check = IntegrityCheck(company_id=company_id, check_type="completeness", entity_type=entity_type, passed=passed, details=f"Expected: {expected_count}, Actual: {actual_count}")
    _checks[company_id].append(check)
    return {"passed": passed, "expected": expected_count, "actual": actual_count, "missing": expected_count - actual_count}

@app.get("/report/{company_id}", response_model=IntegrityReport)
async def get_report(company_id: str):
    checks = _checks.get(company_id, [])
    passed = sum(1 for c in checks if c.passed)
    failed = len(checks) - passed
    return IntegrityReport(company_id=company_id, total_checks=len(checks), passed=passed, failed=failed, pass_rate=passed / max(1, len(checks)) * 100, checks=checks)

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
