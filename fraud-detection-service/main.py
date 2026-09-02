"""
Vimbai Fraud Detection Service
Real-time fraud detection using rule-based scoring, anomaly detection, and risk assessment.
Port: 8312
"""

import os
import uuid
from collections import defaultdict
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "fraud-detection-service"
SERVICE_VERSION = "2.0.0"
PORT = int(os.getenv("PORT", "8312"))
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8001")

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

app = FastAPI(
    title="Vimbai Fraud Detection Service",
    description="Real-time fraud detection with rule-based scoring and anomaly detection",
    version=SERVICE_VERSION,
    docs_url="/docs",
    redoc_url="/redoc",
)
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

# Distributed tracing
try:
    from shared.tracing import get_tracer, setup_tracing

    TRACER = setup_tracing(service_name="fraud-detection-service", instrument_app=app)
except ImportError:
    TRACER = None


# ============================================================
# Enums
# ============================================================


class FraudSeverity(str, Enum):
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"


class FraudStatus(str, Enum):
    PENDING = "pending"
    CONFIRMED = "confirmed"
    FALSE_POSITIVE = "false_positive"
    UNDER_REVIEW = "under_review"


class RiskLevel(str, Enum):
    MINIMAL = "minimal"
    LOW = "low"
    MODERATE = "moderate"
    HIGH = "high"
    EXTREME = "extreme"


# ============================================================
# Models
# ============================================================


class Transaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    account_id: str
    amount: float
    currency: str = "USD"
    timestamp: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    description: str = ""
    merchant: str = ""
    category: str = ""
    is_debit: bool = True
    reference: str = ""
    location: str = ""
    ip_address: str = ""


class FraudRule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str
    rule_type: str  # amount_threshold, frequency, velocity, duplicate, unusual_location, off_hours
    parameters: Dict[str, Any] = {}
    severity: FraudSeverity = FraudSeverity.MEDIUM
    enabled: bool = True


class FraudAlert(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    transaction_id: str
    company_id: str
    rule_id: str
    rule_name: str
    severity: FraudSeverity
    risk_score: float  # 0-100
    description: str
    detected_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    status: FraudStatus = FraudStatus.PENDING
    details: Dict[str, Any] = {}


class RiskAssessment(BaseModel):
    company_id: str
    overall_risk_level: RiskLevel
    risk_score: float  # 0-100
    total_transactions: int
    flagged_transactions: int
    alerts: List[FraudAlert] = []
    assessed_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class FraudDetectionRequest(BaseModel):
    company_id: str
    transactions: List[Transaction]


class FraudDetectionResponse(BaseModel):
    company_id: str
    transactions_analyzed: int
    fraudulent_detected: int
    alerts: List[FraudAlert]
    risk_assessment: RiskAssessment


# ============================================================
# Default Fraud Rules
# ============================================================

DEFAULT_RULES: List[FraudRule] = [
    FraudRule(
        name="Large Transaction Alert",
        description="Flags transactions above a configurable amount threshold",
        rule_type="amount_threshold",
        parameters={"threshold": 50000.0},
        severity=FraudSeverity.HIGH,
    ),
    FraudRule(
        name="High Frequency Alert",
        description="Flags when more than N transactions occur within a time window",
        rule_type="frequency",
        parameters={"max_count": 20, "window_minutes": 60},
        severity=FraudSeverity.MEDIUM,
    ),
    FraudRule(
        name="Velocity Check",
        description="Flags when total transaction value exceeds limit in a time window",
        rule_type="velocity",
        parameters={"max_value": 100000.0, "window_minutes": 30},
        severity=FraudSeverity.HIGH,
    ),
    FraudRule(
        name="Duplicate Transaction",
        description="Flags identical transactions (same amount, merchant, description) within a window",
        rule_type="duplicate",
        parameters={"window_minutes": 15},
        severity=FraudSeverity.MEDIUM,
    ),
    FraudRule(
        name="Round Amount Alert",
        description="Flags unusually round transaction amounts that may indicate fabricated entries",
        rule_type="round_amount",
        parameters={"divisor": 10000.0, "min_amount": 1000.0},
        severity=FraudSeverity.LOW,
    ),
    FraudRule(
        name="Off-Hours Transaction",
        description="Flags transactions occurring outside business hours",
        rule_type="off_hours",
        parameters={"business_start": 8, "business_end": 18},
        severity=FraudSeverity.LOW,
    ),
]


# ============================================================
# In-Memory Storage
# ============================================================

_alerts_store: Dict[str, List[FraudAlert]] = defaultdict(list)
_rules_store: Dict[str, List[FraudRule]] = {}


def get_rules(company_id: str) -> List[FraudRule]:
    if company_id not in _rules_store:
        _rules_store[company_id] = list(DEFAULT_RULES)
    return _rules_store[company_id]


# ============================================================
# Fraud Detection Engine
# ============================================================


def check_amount_threshold(tx: Transaction, rule: FraudRule) -> Optional[FraudAlert]:
    threshold = rule.parameters.get("threshold", 50000.0)
    if abs(tx.amount) >= threshold:
        return FraudAlert(
            transaction_id=tx.id,
            company_id=tx.company_id,
            rule_id=rule.id,
            rule_name=rule.name,
            severity=rule.severity,
            risk_score=min(100.0, (abs(tx.amount) / threshold) * 60),
            description=f"Transaction amount {tx.amount:.2f} {tx.currency} exceeds threshold {threshold:.2f}",
            details={"amount": tx.amount, "threshold": threshold},
        )
    return None


def check_frequency(
    transactions: List[Transaction], tx: Transaction, rule: FraudRule, company_id: str
) -> Optional[FraudAlert]:
    max_count = rule.parameters.get("max_count", 20)
    window = rule.parameters.get("window_minutes", 60)
    window_start = tx.timestamp
    from datetime import timedelta

    count = sum(
        1
        for t in transactions
        if t.company_id == company_id and abs((t.timestamp - window_start).total_seconds()) <= window * 60
    )
    if count > max_count:
        return FraudAlert(
            transaction_id=tx.id,
            company_id=company_id,
            rule_id=rule.id,
            rule_name=rule.name,
            severity=rule.severity,
            risk_score=min(100.0, (count / max_count) * 50),
            description=f"{count} transactions within {window} minutes exceeds limit of {max_count}",
            details={"count": count, "max_count": max_count, "window_minutes": window},
        )
    return None


def check_velocity(
    transactions: List[Transaction], tx: Transaction, rule: FraudRule, company_id: str
) -> Optional[FraudAlert]:
    max_value = rule.parameters.get("max_value", 100000.0)
    window = rule.parameters.get("window_minutes", 30)
    window_start = tx.timestamp
    from datetime import timedelta

    total = sum(
        abs(t.amount)
        for t in transactions
        if t.company_id == company_id and abs((t.timestamp - window_start).total_seconds()) <= window * 60
    )
    if total > max_value:
        return FraudAlert(
            transaction_id=tx.id,
            company_id=company_id,
            rule_id=rule.id,
            rule_name=rule.name,
            severity=rule.severity,
            risk_score=min(100.0, (total / max_value) * 60),
            description=f"Transaction velocity {total:.2f} exceeds limit {max_value:.2f} within {window} min",
            details={"total_value": total, "max_value": max_value, "window_minutes": window},
        )
    return None


def check_duplicate(transactions: List[Transaction], tx: Transaction, rule: FraudRule) -> Optional[FraudAlert]:
    window = rule.parameters.get("window_minutes", 15)
    from datetime import timedelta

    for t in transactions:
        if t.id == tx.id:
            continue
        if (
            t.amount == tx.amount
            and t.merchant == tx.merchant
            and t.description == tx.description
            and abs((t.timestamp - tx.timestamp).total_seconds()) <= window * 60
        ):
            return FraudAlert(
                transaction_id=tx.id,
                company_id=tx.company_id,
                rule_id=rule.id,
                rule_name=rule.name,
                severity=rule.severity,
                risk_score=55.0,
                description=f"Duplicate transaction: same amount ({tx.amount}), merchant ({tx.merchant}), within {window} minutes",
                details={"original_transaction_id": t.id, "window_minutes": window},
            )
    return None


def check_round_amount(tx: Transaction, rule: FraudRule) -> Optional[FraudAlert]:
    divisor = rule.parameters.get("divisor", 10000.0)
    min_amount = rule.parameters.get("min_amount", 1000.0)
    amount = abs(tx.amount)
    if amount >= min_amount and divisor > 0 and amount % divisor == 0:
        return FraudAlert(
            transaction_id=tx.id,
            company_id=tx.company_id,
            rule_id=rule.id,
            rule_name=rule.name,
            severity=rule.severity,
            risk_score=30.0,
            description=f"Round amount {amount:.2f} divisible by {divisor:.0f} may indicate fabricated entry",
            details={"amount": amount, "divisor": divisor},
        )
    return None


def check_off_hours(tx: Transaction, rule: FraudRule) -> Optional[FraudAlert]:
    business_start = rule.parameters.get("business_start", 8)
    business_end = rule.parameters.get("business_end", 18)
    hour = tx.timestamp.hour
    if hour < business_start or hour >= business_end:
        return FraudAlert(
            transaction_id=tx.id,
            company_id=tx.company_id,
            rule_id=rule.id,
            rule_name=rule.name,
            severity=rule.severity,
            risk_score=20.0,
            description=f"Transaction at {hour:02d}:00 outside business hours ({business_start}:00-{business_end}:00)",
            details={"hour": hour, "business_start": business_start, "business_end": business_end},
        )
    return None


def evaluate_transaction(
    tx: Transaction, all_transactions: List[Transaction], rules: List[FraudRule]
) -> List[FraudAlert]:
    """Run all enabled rules against a single transaction."""
    alerts = []
    for rule in rules:
        if not rule.enabled:
            continue
        alert = None
        if rule.rule_type == "amount_threshold":
            alert = check_amount_threshold(tx, rule)
        elif rule.rule_type == "frequency":
            alert = check_frequency(all_transactions, tx, rule, tx.company_id)
        elif rule.rule_type == "velocity":
            alert = check_velocity(all_transactions, tx, rule, tx.company_id)
        elif rule.rule_type == "duplicate":
            alert = check_duplicate(all_transactions, tx, rule)
        elif rule.rule_type == "round_amount":
            alert = check_round_amount(tx, rule)
        elif rule.rule_type == "off_hours":
            alert = check_off_hours(tx, rule)
        if alert:
            alerts.append(alert)
    return alerts


def calculate_risk_level(score: float) -> RiskLevel:
    if score < 20:
        return RiskLevel.MINIMAL
    elif score < 40:
        return RiskLevel.LOW
    elif score < 60:
        return RiskLevel.MODERATE
    elif score < 80:
        return RiskLevel.HIGH
    return RiskLevel.EXTREME


# ============================================================
# API Endpoints
# ============================================================


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.get("/health")
async def health():
    return {"status": "healthy", "rules_loaded": len(DEFAULT_RULES)}


@app.post("/detect", response_model=FraudDetectionResponse)
async def detect_fraud(request: FraudDetectionRequest):
    """Analyze a batch of transactions for fraud indicators."""
    if not request.transactions:
        raise HTTPException(status_code=400, detail="No transactions provided")

    rules = get_rules(request.company_id)
    all_alerts: List[FraudAlert] = []

    for tx in request.transactions:
        tx_alerts = evaluate_transaction(tx, request.transactions, rules)
        all_alerts.extend(tx_alerts)

    # Store alerts
    _alerts_store[request.company_id].extend(all_alerts)

    # Calculate overall risk
    max_score = max((a.risk_score for a in all_alerts), default=0)
    risk_level = calculate_risk_level(max_score)

    risk_assessment = RiskAssessment(
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

    return FraudDetectionResponse(
        company_id=request.company_id,
        transactions_analyzed=len(request.transactions),
        fraudulent_detected=len(set(a.transaction_id for a in all_alerts)),
        alerts=all_alerts,
        risk_assessment=risk_assessment,
    )


@app.get("/alerts/{company_id}")
async def get_alerts(company_id: str, status_filter: Optional[str] = None):
    """Get all fraud alerts for a company, optionally filtered by status."""
    alerts = _alerts_store.get(company_id, [])
    if status_filter:
        alerts = [a for a in alerts if a.status.value == status_filter]
    return {"company_id": company_id, "alerts": alerts, "total": len(alerts)}


@app.put("/alerts/{alert_id}/status")
async def update_alert_status(alert_id: str, new_status: FraudStatus):
    """Update the status of a fraud alert (confirm, dismiss, or mark for review)."""
    for company_alerts in _alerts_store.values():
        for alert in company_alerts:
            if alert.id == alert_id:
                alert.status = new_status
                logger.info("alert_status_updated", alert_id=alert_id, new_status=new_status.value)
                return {"alert_id": alert_id, "status": new_status.value}
    raise HTTPException(status_code=404, detail="Alert not found")


@app.get("/rules/{company_id}")
async def get_fraud_rules(company_id: str):
    """Get all fraud detection rules for a company."""
    return {"company_id": company_id, "rules": get_rules(company_id)}


@app.post("/rules/{company_id}")
async def add_fraud_rule(company_id: str, rule: FraudRule):
    """Add a new fraud detection rule."""
    rules = get_rules(company_id)
    rules.append(rule)
    logger.info("rule_added", company_id=company_id, rule_name=rule.name)
    return {"rule_id": rule.id, "name": rule.name, "status": "added"}


@app.put("/rules/{rule_id}")
async def toggle_rule(rule_id: str, enabled: bool):
    """Enable or disable a fraud detection rule."""
    for rules in _rules_store.values():
        for rule in rules:
            if rule.id == rule_id:
                rule.enabled = enabled
                return {"rule_id": rule_id, "enabled": enabled}
    raise HTTPException(status_code=404, detail="Rule not found")


@app.get("/risk/{company_id}")
async def get_risk_assessment(company_id: str):
    """Get current risk assessment for a company based on stored alerts."""
    alerts = _alerts_store.get(company_id, [])
    if not alerts:
        return RiskAssessment(
            company_id=company_id,
            overall_risk_level=RiskLevel.MINIMAL,
            risk_score=0,
            total_transactions=0,
            flagged_transactions=0,
            alerts=[],
        )
    max_score = max(a.risk_score for a in alerts)
    return RiskAssessment(
        company_id=company_id,
        overall_risk_level=calculate_risk_level(max_score),
        risk_score=max_score,
        total_transactions=len(set(a.transaction_id for a in alerts)),
        flagged_transactions=len(set(a.transaction_id for a in alerts)),
        alerts=alerts,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
