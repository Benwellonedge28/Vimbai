"""Pydantic models, enums and default rules for the Fraud Detection Service."""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List

from pydantic import BaseModel, Field


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
