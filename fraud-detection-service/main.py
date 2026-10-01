"""Vimbai Fraud Detection Service - transaction fraud analysis, alerts and detection rules. Port: 8370

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "fraud_detection_service" not in _sys.modules or not hasattr(
    _sys.modules.get("fraud_detection_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("fraud_detection_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["fraud_detection_service"] = _pkg
    _sys.modules["fraud_detection_service"].__path__ = [_HERE]

import os

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from fraud_detection_service import crud, models
from fraud_detection_service.dependencies import book_id_var, get_db_session, get_user_id
from fraud_detection_service.engine import calculate_risk_level, evaluate_transaction
from fraud_detection_service.exceptions import FraudDetectionError
from neo4j import AsyncSession

SERVICE_NAME = "fraud-detection-service"
PORT = int(os.getenv("PORT", "8370"))
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
app = FastAPI(title="Vimbai Fraud Detection Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
# Distributed tracing (OpenTelemetry)
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(FraudDetectionError)
async def _fraud_detection_error(request: Request, exc: FraudDetectionError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/detect", response_model=models.FraudDetectionResponse)
async def detect_fraud(
    request: models.FraudDetectionRequest,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Analyze a batch of transactions for fraud indicators using the caller's Book-visible rules."""
    if not request.transactions:
        raise HTTPException(status_code=400, detail="No transactions provided")

    rules = await crud.get_or_seed_rules(db_session, user_id, request.company_id)
    all_alerts = []
    for tx in request.transactions:
        all_alerts.extend(evaluate_transaction(tx, request.transactions, rules))

    # Store alerts
    await crud.store_alerts(db_session, user_id, all_alerts)

    # Calculate overall risk
    max_score = max((a.risk_score for a in all_alerts), default=0)
    risk_level = calculate_risk_level(max_score)

    risk_assessment = models.RiskAssessment(
        company_id=request.company_id,
        overall_risk_level=risk_level,
        risk_score=max_score,
        total_transactions=len(request.transactions),
        flagged_transactions=len(set(a.transaction_id for a in all_alerts)),
        alerts=all_alerts,
    )

    logger.info(
        "fraud_detection_complete",
        company_id=request.company_id,
        transactions=len(request.transactions),
        alerts=len(all_alerts),
        risk_level=risk_level.value,
        risk_score=max_score,
    )

    return models.FraudDetectionResponse(
        company_id=request.company_id,
        transactions_analyzed=len(request.transactions),
        fraudulent_detected=len(set(a.transaction_id for a in all_alerts)),
        alerts=all_alerts,
        risk_assessment=risk_assessment,
    )


@app.get("/alerts/{company_id}")
async def get_alerts(
    company_id: str,
    status_filter: str = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's fraud alerts for a company, optionally filtered by status."""
    alerts = await crud.list_alerts(db_session, user_id, company_id, status_filter)
    return {"company_id": company_id, "alerts": alerts, "total": len(alerts)}


@app.put("/alerts/{alert_id}/status")
async def update_alert_status(
    alert_id: str,
    new_status: models.FraudStatus,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update the status of a fraud alert (confirm, dismiss, or mark for review)."""
    try:
        result = await crud.update_alert_status(db_session, user_id, alert_id, new_status)
        logger.info("alert_status_updated", alert_id=alert_id, new_status=new_status.value)
        return result
    except FraudDetectionError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.get("/rules/{company_id}")
async def get_fraud_rules(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's fraud detection rules for a company (seeded with defaults on first access)."""
    rules = await crud.get_or_seed_rules(db_session, user_id, company_id)
    return {"company_id": company_id, "rules": rules}


@app.post("/rules/{company_id}")
async def add_fraud_rule(
    company_id: str,
    rule: models.FraudRule,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add a new fraud detection rule for the caller's company scope."""
    item = await crud.add_rule(db_session, user_id, company_id, rule)
    logger.info("rule_added", company_id=company_id, rule_name=rule.name)
    return {"rule_id": item.id, "name": item.name, "status": "added"}


@app.put("/rules/{rule_id}")
async def toggle_rule(
    rule_id: str,
    enabled: bool,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Enable or disable a fraud detection rule owned by the caller."""
    try:
        return await crud.toggle_rule(db_session, user_id, rule_id, enabled)
    except FraudDetectionError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.get("/risk/{company_id}", response_model=models.RiskAssessment)
async def get_risk_assessment(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get current risk assessment for a company based on the caller's stored alerts."""
    return await crud.get_risk_assessment(db_session, user_id, company_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
