"""
FinAcc Suspense Account & Error Detection Service
Detects, tracks, and helps resolve accounting errors including:
- Unbalanced journal entries
- Missing information
- Incorrect account classification
- Duplicate entries
- Period errors
- Suspense account management
- Error correction workflows
- User notifications for suspense account existence
"""

from fastapi import FastAPI, HTTPException, Depends, BackgroundTasks, Request
from pydantic import BaseModel, Field, field_validator
from typing import List, Optional, Dict, Any, Literal
from datetime import datetime, timezone, timedelta
from enum import Enum
import uuid
import hashlib
import json
from decimal import Decimal

app = FastAPI(
    title="FinAcc Suspense Account & Error Detection Service",
    description="Comprehensive accounting error detection, suspense account management, and error correction workflows",
    version="1.0.0",
)

# ============================================================================
# Enums
# ============================================================================

class ErrorType(str, Enum):
    UNBALANCED_ENTRY = "unbalanced_entry"
    MISSING_INFORMATION = "missing_information"
    WRONG_ACCOUNT = "wrong_account"
    DUPLICATE_ENTRY = "duplicate_entry"
    PERIOD_ERROR = "period_error"
    ROUNDING_ERROR = "rounding_error"
    CLASSIFICATION_ERROR = "classification_error"
    AMOUNT_MISMATCH = "amount_mismatch"
    CURRENCY_ERROR = "currency_error"
    UNAUTHORIZED_ENTRY = "unauthorized_entry"
    SUSPENSE_PLACEMENT = "suspense_placement"
    RECONCILIATION_ERROR = "reconciliation_error"


class ErrorSeverity(str, Enum):
    CRITICAL = "critical"
    HIGH = "high"
    MEDIUM = "medium"
    LOW = "low"
    INFO = "info"


class ErrorStatus(str, Enum):
    DETECTED = "detected"
    REVIEWING = "reviewing"
    PENDING_CORRECTION = "pending_correction"
    CORRECTION_APPLIED = "correction_applied"
    DISMISSED = "dismissed"
    ESCALATED = "escalated"


class SuspenseAccountStatus(str, Enum):
    ACTIVE = "active"
    PARTIALLY_CLEARED = "partially_cleared"
    CLEARED = "cleared"
    AGED = "aged"
    WRITE_OFF_PENDING = "write_off_pending"


class CorrectionType(str, Enum):
    JOURNAL_REVERSAL = "journal_reversal"
    ADJUSTING_ENTRY = "adjusting_entry"
    RECLASSIFICATION = "reclassification"
    SUSPENSE_CLEARANCE = "suspense_clearance"
    WRITE_OFF = "write_off"
    MANUAL_ADJUSTMENT = "manual_adjustment"


# ============================================================================
# Pydantic Models
# ============================================================================

class AccountingError(BaseModel):
    id: str
    error_type: ErrorType
    severity: ErrorSeverity
    status: ErrorStatus
    entry_id: Optional[str] = None
    entry_date: Optional[datetime] = None
    journal_entry_id: Optional[str] = None
    account_number: Optional[str] = None
    detected_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    detected_by: str = "system"
    description: str
    details: Dict[str, Any] = {}
    affected_accounts: List[str] = []
    amount: Optional[Decimal] = None
    variance: Optional[Decimal] = None
    variance_percentage: Optional[float] = None
    source: str  # manual_entry, import, automated, reconciliation
    resolved_at: Optional[datetime] = None
    resolved_by: Optional[str] = None
    correction_entry_id: Optional[str] = None


class SuspenseAccountEntry(BaseModel):
    id: str
    suspense_account_code: str
    original_entry_id: Optional[str] = None
    error_id: Optional[str] = None
    amount: Decimal
    debit: bool
    entry_date: datetime
    description: str
    reason: str  # balancing, missing_info, pending_verification
    status: SuspenseAccountStatus = SuspenseAccountStatus.ACTIVE
    cleared_at: Optional[datetime] = None
    cleared_by: Optional[str] = None
    clearance_entry_id: Optional[str] = None
    aging_days: int = 0
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class SuspenseAccountSummary(BaseModel):
    account_code: str
    account_name: str
    status: SuspenseAccountStatus
    total_debit: Decimal
    total_credit: Decimal
    net_balance: Decimal
    entry_count: int
    oldest_entry_days: int
    oldest_entry_date: Optional[datetime] = None
    requires_attention: bool
    alerts: List[str] = []
    clearance_recommendations: List[str] = []


class ErrorCorrection(BaseModel):
    id: str
    error_id: str
    correction_type: CorrectionType
    original_entry_id: str
    correction_entry_id: Optional[str] = None
    correction_journal_entry_id: Optional[str] = None
    description: str
    journal_lines: List[Dict[str, Any]] = []
    approved_by: Optional[str] = None
    approved_at: Optional[datetime] = None
    status: Literal["pending", "approved", "rejected", "applied", "failed"]
    applied_at: Optional[datetime] = None
    applied_by: Optional[str] = None
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ErrorDetectionRule(BaseModel):
    id: str
    name: str
    description: str
    error_type: ErrorType
    severity: ErrorSeverity
    enabled: bool = True
    conditions: Dict[str, Any] = {}
    actions: List[str] = []  # notify_user, create_suspense, block_entry
    notification_channels: List[str] = []
    auto_create_suspense: bool = False
    auto_approve_threshold: Optional[Decimal] = None


class Notification(BaseModel):
    id: str
    user_id: str
    user_email: str
    organization_id: str
    type: str  # suspense_alert, error_detected, correction_needed, clearance_complete
    title: str
    message: str
    priority: str = "normal"
    related_error_id: Optional[str] = None
    related_suspense_id: Optional[str] = None
    action_required: bool = False
    action_url: Optional[str] = None
    read: bool = False
    read_at: Optional[datetime] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ValidationResult(BaseModel):
    valid: bool
    errors: List[Dict[str, Any]] = []
    warnings: List[Dict[str, Any]] = []
    suspense_required: bool = False
    suspense_amount: Optional[Decimal] = None
    journal_entry_valid: bool = True
    balanced: bool = True
    total_debits: Optional[Decimal] = None
    total_credits: Optional[Decimal] = None
    variance: Optional[Decimal] = None


# ============================================================================
# Storage
# ============================================================================

accounting_errors: Dict[str, AccountingError] = {}
suspense_entries: Dict[str, SuspenseAccountEntry] = {}
error_corrections: Dict[str, ErrorCorrection] = {}
detection_rules: Dict[str, ErrorDetectionRule] = {}
notifications: Dict[str, Notification] = {}
suspense_accounts: Dict[str, SuspenseAccountSummary] = {}

# ============================================================================
# Default Detection Rules
# ============================================================================

DEFAULT_DETECTION_RULES = {
    "unbalanced_entry": ErrorDetectionRule(
        id="rule_unbalanced",
        name="Unbalanced Journal Entry Detection",
        description="Detects journal entries where debits don't equal credits",
        error_type=ErrorType.UNBALANCED_ENTRY,
        severity=ErrorSeverity.HIGH,
        enabled=True,
        conditions={"max_variance": Decimal("0.01")},
        actions=["notify_user", "create_suspense"],
        notification_channels=["email", "dashboard"],
        auto_create_suspense=True,
    ),
    "missing_information": ErrorDetectionRule(
        id="rule_missing_info",
        name="Missing Information Detection",
        description="Detects entries with missing required fields",
        error_type=ErrorType.MISSING_INFORMATION,
        severity=ErrorSeverity.MEDIUM,
        enabled=True,
        conditions={"required_fields": ["description", "reference", "date"]},
        actions=["notify_user", "block_entry"],
        notification_channels=["dashboard"],
        auto_create_suspense=False,
    ),
    "duplicate_entry": ErrorDetectionRule(
        id="rule_duplicate",
        name="Duplicate Entry Detection",
        description="Detects potential duplicate journal entries",
        error_type=ErrorType.DUPLICATE_ENTRY,
        severity=ErrorSeverity.HIGH,
        enabled=True,
        conditions={"hash_similarity": 0.95},
        actions=["notify_user"],
        notification_channels=["email", "dashboard"],
        auto_create_suspense=False,
    ),
    "period_error": ErrorDetectionRule(
        id="rule_period",
        name="Period Boundary Error Detection",
        description="Detects entries posted to wrong accounting periods",
        error_type=ErrorType.PERIOD_ERROR,
        severity=ErrorSeverity.MEDIUM,
        enabled=True,
        conditions={"check_future_dates": True, "check_locked_periods": True},
        actions=["notify_user", "require_approval"],
        notification_channels=["email"],
        auto_create_suspense=False,
    ),
    "suspense_aging": ErrorDetectionRule(
        id="rule_suspense_aging",
        name="Suspense Account Aging Alert",
        description="Alerts when suspense entries age beyond threshold",
        error_type=ErrorType.SUSPENSE_PLACEMENT,
        severity=ErrorSeverity.HIGH,
        enabled=True,
        conditions={"aging_threshold_days": 30},
        actions=["notify_user", "escalate"],
        notification_channels=["email", "dashboard", "sms"],
        auto_create_suspense=False,
    ),
}

detection_rules = DEFAULT_DETECTION_RULES.copy()


# ============================================================================
# Helper Functions
# ============================================================================

def calculate_hash(entry_data: Dict) -> str:
    """Calculate hash for duplicate detection"""
    content = json.dumps(entry_data, sort_keys=True, default=str)
    return hashlib.sha256(content.encode()).hexdigest()


def calculate_variance_percentage(debit: Decimal, credit: Decimal) -> float:
    """Calculate variance percentage between debit and credit"""
    if debit == 0 and credit == 0:
        return 0.0
    total = debit + credit
    if total == 0:
        return 0.0
    return float(abs(debit - credit) / total * 100)


def get_aging_days(entry_date: datetime) -> int:
    """Calculate aging in days from entry date"""
    now = datetime.now(timezone.utc)
    if entry_date.tzinfo is None:
        entry_date = entry_date.replace(tzinfo=timezone.utc)
    return (now - entry_date).days


# ============================================================================
# API Endpoints
# ============================================================================

@app.get("/")
async def health_check():
    """Health check endpoint"""
    total_errors = len(accounting_errors)
    active_errors = sum(1 for e in accounting_errors.values() if e.status not in [ErrorStatus.CORRECTION_APPLIED, ErrorStatus.DISMISSED])
    suspense_balance = sum(
        (e.amount if e.debit else Decimal("0")) - (e.amount if not e.debit else Decimal("0"))
        for e in suspense_entries.values()
    )

    return {
        "status": "healthy",
        "service": "suspense-error",
        "version": "1.0.0",
        "total_errors": total_errors,
        "active_errors": active_errors,
        "suspense_entries": len(suspense_entries),
        "suspense_net_balance": str(suspense_balance),
    }


# --- Journal Entry Validation ---

@app.post("/validate/journal-entry")
async def validate_journal_entry(entry: Dict[str, Any]):
    """
    Validate a journal entry for errors before posting.
    Returns validation result with any detected issues.
    """
    result = ValidationResult(
        valid=True,
        errors=[],
        warnings=[],
        suspense_required=False,
    )

    # Check if entry has lines
    if "lines" not in entry or not entry["lines"]:
        result.valid = False
        result.journal_entry_valid = False
        result.errors.append({
            "code": "MISSING_LINES",
            "message": "Journal entry must have at least one line",
            "field": "lines",
        })
        return result

    # Calculate totals
    total_debits = Decimal("0")
    total_credits = Decimal("0")

    for i, line in enumerate(entry["lines"]):
        amount = Decimal(str(line.get("amount", 0)))
        if line.get("debit", False):
            total_debits += amount
        else:
            total_credits += amount

    result.total_debits = total_debits
    result.total_credits = total_credits

    # Check balance
    variance = abs(total_debits - total_credits)
    result.variance = variance
    result.balanced = variance <= Decimal("0.01")

    if not result.balanced:
        result.valid = False
        result.journal_entry_valid = False
        result.suspense_required = True
        result.suspense_amount = variance

        result.errors.append({
            "code": "UNBALANCED_ENTRY",
            "message": f"Journal entry is not balanced. Debits: {total_debits}, Credits: {total_credits}, Variance: {variance}",
            "variance": str(variance),
            "suggestion": "Add a suspense account entry or correct the amounts",
            "variance_percentage": calculate_variance_percentage(total_debits, total_credits),
        })

    # Check for required fields
    required_fields = ["date", "description", "reference"]
    for field in required_fields:
        if field not in entry or not entry[field]:
            result.warnings.append({
                "code": "MISSING_FIELD",
                "message": f"Recommended field '{field}' is missing",
                "field": field,
            })

    # Check for duplicate potential (hash-based)
    entry_hash = calculate_hash({
        "date": entry.get("date"),
        "description": entry.get("description"),
        "amount": str(total_debits + total_credits),
        "lines": [
            {"account": l.get("account"), "amount": l.get("amount")}
            for l in entry.get("lines", [])
        ],
    })

    # Check for date in future
    entry_date = entry.get("date")
    if entry_date:
        if isinstance(entry_date, str):
            try:
                entry_date = datetime.fromisoformat(entry_date.replace("Z", "+00:00"))
            except:
                pass

        if isinstance(entry_date, datetime) and entry_date > datetime.now(timezone.utc):
            result.warnings.append({
                "code": "FUTURE_DATE",
                "message": "Journal entry has a future date",
                "date": str(entry_date),
            })

    return result


@app.post("/validate/account")
async def validate_account_entry(entry: Dict[str, Any]):
    """Validate individual account entry"""
    errors = []
    warnings = []

    # Check account code format
    account_code = entry.get("account_code", "")
    if not account_code:
        errors.append({
            "code": "MISSING_ACCOUNT",
            "message": "Account code is required",
        })

    # Check amount
    amount = entry.get("amount", 0)
    try:
        amount = Decimal(str(amount))
        if amount <= 0:
            errors.append({
                "code": "INVALID_AMOUNT",
                "message": "Amount must be greater than zero",
            })
    except:
        errors.append({
            "code": "INVALID_AMOUNT",
            "message": "Amount must be a valid number",
        })

    return {
        "valid": len(errors) == 0,
        "errors": errors,
        "warnings": warnings,
    }


# --- Error Detection ---

@app.post("/errors/detect")
async def detect_errors(entry_data: Dict[str, Any], user_id: str = "system"):
    """Detect errors in journal entry data"""
    errors_found = []

    # Check for unbalanced entry
    validation = await validate_journal_entry(entry_data)
    if not validation.balanced:
        error = AccountingError(
            id=str(uuid.uuid4()),
            error_type=ErrorType.UNBALANCED_ENTRY,
            severity=ErrorSeverity.HIGH,
            status=ErrorStatus.DETECTED,
            description=f"Unbalanced journal entry detected. Variance: {validation.variance}",
            details={
                "total_debits": str(validation.total_debits),
                "total_credits": str(validation.total_credits),
                "variance": str(validation.variance),
                "variance_percentage": validation.variance_percentage,
            },
            amount=validation.variance,
            variance=validation.variance,
            variance_percentage=validation.variance_percentage,
            source="journal_entry_validation",
            detected_by=user_id,
        )
        if "entry_id" in entry_data:
            error.entry_id = entry_data["entry_id"]
        if "date" in entry_data:
            error.entry_date = entry_data["date"]

        errors_found.append(error)

    # Check for missing information
    required_fields = ["description", "reference"]
    missing_fields = [f for f in required_fields if f not in entry_data or not entry_data[f]]
    if missing_fields:
        error = AccountingError(
            id=str(uuid.uuid4()),
            error_type=ErrorType.MISSING_INFORMATION,
            severity=ErrorSeverity.MEDIUM,
            status=ErrorStatus.DETECTED,
            description=f"Missing required fields: {', '.join(missing_fields)}",
            details={"missing_fields": missing_fields},
            source="journal_entry_validation",
            detected_by=user_id,
        )
        errors_found.append(error)

    # Check for duplicate entry
    entry_hash = calculate_hash(entry_data)
    for existing_error in accounting_errors.values():
        if existing_error.error_type == ErrorType.DUPLICATE_ENTRY:
            if existing_error.details.get("hash") == entry_hash:
                error = AccountingError(
                    id=str(uuid.uuid4()),
                    error_type=ErrorType.DUPLICATE_ENTRY,
                    severity=ErrorSeverity.HIGH,
                    status=ErrorStatus.DETECTED,
                    description="Potential duplicate journal entry detected",
                    details={
                        "original_entry_id": existing_error.entry_id,
                        "hash": entry_hash,
                    },
                    source="duplicate_check",
                    detected_by=user_id,
                )
                errors_found.append(error)
                break

    # Store detected errors
    for error in errors_found:
        accounting_errors[error.id] = error

    return {
        "errors_detected": len(errors_found),
        "errors": errors_found,
        "requires_attention": any(e.severity in [ErrorSeverity.CRITICAL, ErrorSeverity.HIGH] for e in errors_found),
    }


@app.get("/errors")
async def list_errors(
    status: Optional[ErrorStatus] = None,
    error_type: Optional[ErrorType] = None,
    severity: Optional[ErrorSeverity] = None,
    limit: int = 100,
    offset: int = 0
):
    """List accounting errors with filters"""
    results = list(accounting_errors.values())

    if status:
        results = [e for e in results if e.status == status]
    if error_type:
        results = [e for e in results if e.error_type == error_type]
    if severity:
        results = [e for e in results if e.severity == severity]

    results.sort(key=lambda x: x.detected_at, reverse=True)
    return {
        "total": len(results),
        "errors": results[offset:offset + limit],
    }


@app.get("/errors/{error_id}")
async def get_error(error_id: str):
    """Get specific error details"""
    if error_id not in accounting_errors:
        raise HTTPException(status_code=404, detail="Error not found")
    return accounting_errors[error_id]


@app.put("/errors/{error_id}/status")
async def update_error_status(
    error_id: str,
    status: ErrorStatus,
    updated_by: str = "admin",
    notes: Optional[str] = None
):
    """Update error status"""
    if error_id not in accounting_errors:
        raise HTTPException(status_code=404, detail="Error not found")

    error = accounting_errors[error_id]
    error.status = status

    if status == ErrorStatus.CORRECTION_APPLIED:
        error.resolved_at = datetime.now(timezone.utc)
        error.resolved_by = updated_by

    return error


@app.delete("/errors/{error_id}")
async def dismiss_error(error_id: str, reason: str, dismissed_by: str = "admin"):
    """Dismiss an error with reason"""
    if error_id not in accounting_errors:
        raise HTTPException(status_code=404, detail="Error not found")

    error = accounting_errors[error_id]
    error.status = ErrorStatus.DISMISSED
    error.details["dismissed_reason"] = reason
    error.details["dismissed_by"] = dismissed_by
    error.resolved_at = datetime.now(timezone.utc)
    error.resolved_by = dismissed_by

    return {"status": "dismissed", "error_id": error_id}


# --- Suspense Account Management ---

@app.get("/suspense/accounts")
async def list_suspense_accounts():
    """List all suspense accounts and their status"""
    return list(suspense_accounts.values())


@app.get("/suspense/accounts/{account_code}")
async def get_suspense_account(account_code: str):
    """Get suspense account details"""
    if account_code not in suspense_accounts:
        raise HTTPException(status_code=404, detail="Suspense account not found")
    return suspense_accounts[account_code]


@app.get("/suspense/entries")
async def list_suspense_entries(
    account_code: Optional[str] = None,
    status: Optional[SuspenseAccountStatus] = None,
    limit: int = 100
):
    """List suspense account entries"""
    results = list(suspense_entries.values())

    if account_code:
        results = [e for e in results if e.suspense_account_code == account_code]
    if status:
        results = [e for e in results if e.status == status]

    return {
        "total": len(results),
        "entries": results[:limit],
    }


@app.post("/suspense/entries")
async def create_suspense_entry(entry: SuspenseAccountEntry):
    """Create a suspense account entry"""
    entry.id = str(uuid.uuid4())
    entry.created_at = datetime.now(timezone.utc)
    entry.aging_days = get_aging_days(entry.entry_date)

    suspense_entries[entry.id] = entry

    # Update suspense account summary
    await update_suspense_account_summary(entry.suspense_account_code)

    return entry


@app.post("/suspense/entries/{entry_id}/clear")
async def clear_suspense_entry(
    entry_id: str,
    clearance_entry_id: str,
    cleared_by: str = "admin",
    notes: Optional[str] = None
):
    """Clear a suspense account entry"""
    if entry_id not in suspense_entries:
        raise HTTPException(status_code=404, detail="Suspense entry not found")

    entry = suspense_entries[entry_id]
    entry.status = SuspenseAccountStatus.CLEARED
    entry.cleared_at = datetime.now(timezone.utc)
    entry.cleared_by = cleared_by
    entry.clearance_entry_id = clearance_entry_id

    # Update suspense account summary
    await update_suspense_account_summary(entry.suspense_account_code)

    return entry


@app.get("/suspense/aging-report")
async def get_suspense_aging_report():
    """Generate aging report for suspense accounts"""
    aging_buckets = {
        "0_30_days": [],
        "31_60_days": [],
        "61_90_days": [],
        "over_90_days": [],
    }

    for entry in suspense_entries.values():
        if entry.status == SuspenseAccountStatus.ACTIVE:
            aging = entry.aging_days
            entry_dict = entry.model_dump()

            if aging <= 30:
                aging_buckets["0_30_days"].append(entry_dict)
            elif aging <= 60:
                aging_buckets["31_60_days"].append(entry_dict)
            elif aging <= 90:
                aging_buckets["61_90_days"].append(entry_dict)
            else:
                aging_buckets["over_90_days"].append(entry_dict)

    return {
        "aging_buckets": aging_buckets,
        "total_entries": len(suspense_entries),
        "total_amount": sum(e.amount for e in suspense_entries.values() if e.status == SuspenseAccountStatus.ACTIVE),
        "requires_immediate_action": len(aging_buckets["over_90_days"]) > 0,
    }


@app.get("/suspense/summary")
async def get_suspense_summary():
    """Get overall suspense account summary"""
    total_debits = Decimal("0")
    total_credits = Decimal("0")
    active_count = 0
    oldest_date = None

    for entry in suspense_entries.values():
        if entry.debit:
            total_debits += entry.amount
        else:
            total_credits += entry.amount

        if entry.status == SuspenseAccountStatus.ACTIVE:
            active_count += 1
            if oldest_date is None or entry.entry_date < oldest_date:
                oldest_date = entry.entry_date

    net_balance = total_debits - total_credits

    return {
        "total_debits": str(total_debits),
        "total_credits": str(total_credits),
        "net_balance": str(net_balance),
        "active_entries": active_count,
        "total_entries": len(suspense_entries),
        "oldest_entry_date": oldest_date,
        "oldest_entry_days": get_aging_days(oldest_date) if oldest_date else 0,
        "requires_attention": net_balance != Decimal("0") or active_count > 10,
        "alerts": generate_suspense_alerts(net_balance, active_count, oldest_date),
    }


def generate_suspense_alerts(net_balance: Decimal, active_count: int, oldest_date: Optional[datetime]) -> List[str]:
    """Generate alerts for suspense account status"""
    alerts = []

    if net_balance != Decimal("0"):
        alerts.append(f"Suspense account has unbalanced balance: {net_balance}")

    if active_count > 10:
        alerts.append(f"High number of active suspense entries: {active_count}")

    if oldest_date:
        aging = get_aging_days(oldest_date)
        if aging > 30:
            alerts.append(f"Oldest suspense entry is {aging} days old")
        if aging > 60:
            alerts.append(f"URGENT: Suspense entry requires immediate attention - {aging} days old")
        if aging > 90:
            alerts.append(f"CRITICAL: Suspense entry must be resolved - {aging} days old")

    if not alerts:
        alerts.append("No alerts - suspense account is healthy")

    return alerts


async def update_suspense_account_summary(account_code: str):
    """Update suspense account summary after changes"""
    entries = [e for e in suspense_entries.values() if e.suspense_account_code == account_code]

    if not entries:
        return

    total_debit = sum(e.amount for e in entries if e.debit)
    total_credit = sum(e.amount for e in entries if not e.debit)

    active_entries = [e for e in entries if e.status == SuspenseAccountStatus.ACTIVE]
    oldest_entry = min(entries, key=lambda x: x.entry_date) if entries else None

    status = SuspenseAccountStatus.ACTIVE
    if len(active_entries) == 0:
        status = SuspenseAccountStatus.CLEARED
    elif len(active_entries) < len(entries):
        status = SuspenseAccountStatus.PARTIALLY_CLEARED
    if oldest_entry and get_aging_days(oldest_entry.entry_date) > 90:
        status = SuspenseAccountStatus.AGED

    summary = SuspenseAccountSummary(
        account_code=account_code,
        account_name=f"Suspense Account ({account_code})",
        status=status,
        total_debit=total_debit,
        total_credit=total_credit,
        net_balance=total_debit - total_credit,
        entry_count=len(entries),
        oldest_entry_days=get_aging_days(oldest_entry.entry_date) if oldest_entry else 0,
        oldest_entry_date=oldest_entry.entry_date if oldest_entry else None,
        requires_attention=total_debit != total_credit or len(active_entries) > 5,
        alerts=generate_suspense_alerts(
            total_debit - total_credit,
            len(active_entries),
            oldest_entry.entry_date if oldest_entry else None,
        ),
        clearance_recommendations=generate_clearance_recommendations(entries),
    )

    suspense_accounts[account_code] = summary


def generate_clearance_recommendations(entries: List[SuspenseAccountEntry]) -> List[str]:
    """Generate recommendations to clear suspense entries"""
    recommendations = []

    active_entries = [e for e in entries if e.status == SuspenseAccountStatus.ACTIVE]
    if len(active_entries) == 0:
        recommendations.append("All suspense entries have been cleared")
    else:
        recommendations.append(f"Clear {len(active_entries)} active suspense entries")

    for entry in active_entries:
        if entry.reason == "balancing":
            recommendations.append(f"Review entry {entry.id} - it was created for balancing purposes")
        elif entry.reason == "missing_info":
            recommendations.append(f"Obtain missing information for entry {entry.id}")

    return recommendations[:5]


# --- Error Correction ---

@app.post("/corrections")
async def create_correction(correction: ErrorCorrection):
    """Create error correction"""
    correction.id = str(uuid.uuid4())
    correction.created_at = datetime.now(timezone.utc)
    correction.updated_at = datetime.now(timezone.utc)

    error_corrections[correction.id] = correction

    # Update error status
    if correction.error_id in accounting_errors:
        accounting_errors[correction.error_id].status = ErrorStatus.PENDING_CORRECTION

    return correction


@app.get("/corrections")
async def list_corrections(
    status: Optional[str] = None,
    error_id: Optional[str] = None,
    limit: int = 50
):
    """List error corrections"""
    results = list(error_corrections.values())

    if status:
        results = [e for e in results if e.status == status]
    if error_id:
        results = [e for e in results if e.error_id == error_id]

    return {
        "total": len(results),
        "corrections": results[:limit],
    }


@app.put("/corrections/{correction_id}/approve")
async def approve_correction(
    correction_id: str,
    approved_by: str
):
    """Approve a correction"""
    if correction_id not in error_corrections:
        raise HTTPException(status_code=404, detail="Correction not found")

    correction = error_corrections[correction_id]
    correction.status = "approved"
    correction.approved_by = approved_by
    correction.approved_at = datetime.now(timezone.utc)
    correction.updated_at = datetime.now(timezone.utc)

    return correction


@app.put("/corrections/{correction_id}/apply")
async def apply_correction(
    correction_id: str,
    applied_by: str,
    applied_entry_id: Optional[str] = None
):
    """Apply a correction"""
    if correction_id not in error_corrections:
        raise HTTPException(status_code=404, detail="Correction not found")

    correction = error_corrections[correction_id]
    correction.status = "applied"
    correction.applied_at = datetime.now(timezone.utc)
    correction.applied_by = applied_by
    correction.correction_entry_id = applied_entry_id
    correction.updated_at = datetime.now(timezone.utc)

    # Update error status
    if correction.error_id in accounting_errors:
        error = accounting_errors[correction.error_id]
        error.status = ErrorStatus.CORRECTION_APPLIED
        error.resolved_at = datetime.now(timezone.utc)
        error.resolved_by = applied_by
        error.correction_entry_id = applied_entry_id

    return correction


# --- Notifications ---

@app.get("/notifications")
async def list_notifications(
    user_id: Optional[str] = None,
    unread_only: bool = False,
    limit: int = 50
):
    """List notifications"""
    results = list(notifications.values())

    if user_id:
        results = [n for n in results if n.user_id == user_id]
    if unread_only:
        results = [n for n in results if not n.read]

    results.sort(key=lambda x: x.created_at, reverse=True)
    return {
        "total": len(results),
        "unread_count": sum(1 for n in results if not n.read),
        "notifications": results[:limit],
    }


@app.post("/notifications/suspense-alert")
async def create_suspense_alert(
    user_id: str,
    user_email: str,
    organization_id: str,
    suspense_id: str,
    message: str
):
    """Create suspense account alert notification"""
    notification = Notification(
        id=str(uuid.uuid4()),
        user_id=user_id,
        user_email=user_email,
        organization_id=organization_id,
        type="suspense_alert",
        title="Suspense Account Alert",
        message=message,
        priority="high",
        related_suspense_id=suspense_id,
        action_required=True,
        action_url=f"/suspense/entries/{suspense_id}",
        created_at=datetime.now(timezone.utc),
    )

    notifications[notification.id] = notification
    return notification


@app.put("/notifications/{notification_id}/read")
async def mark_notification_read(notification_id: str):
    """Mark notification as read"""
    if notification_id not in notifications:
        raise HTTPException(status_code=404, detail="Notification not found")

    notification = notifications[notification_id]
    notification.read = True
    notification.read_at = datetime.now(timezone.utc)

    return notification


@app.post("/notifications/check-suspense")
async def check_and_notify_suspense(
    organization_id: str,
    check_user_id: str
):
    """
    Check suspense account status and create notifications
    for any issues requiring attention
    """
    summary = await get_suspense_summary()

    notifications_created = []

    if summary["requires_attention"]:
        for alert in summary["alerts"]:
            if "URGENT" in alert or "CRITICAL" in alert:
                notification = Notification(
                    id=str(uuid.uuid4()),
                    user_id=check_user_id,
                    user_email=f"{check_user_id}@finacc.com",
                    organization_id=organization_id,
                    type="suspense_alert",
                    title="Suspense Account Requires Immediate Attention",
                    message=alert,
                    priority="high",
                    action_required=True,
                    created_at=datetime.now(timezone.utc),
                )
                notifications[notification.id] = notification
                notifications_created.append(notification)

    return {
        "notifications_created": len(notifications_created),
        "notifications": notifications_created,
        "suspense_summary": summary,
    }


# --- Error Analysis ---

@app.get("/errors/analysis")
async def get_error_analysis(
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None
):
    """Get error analysis statistics"""
    errors = list(accounting_errors.values())

    if start_date:
        errors = [e for e in errors if e.detected_at >= start_date]
    if end_date:
        errors = [e in errors if e.detected_at <= end_date]

    # Count by type
    by_type = {}
    by_severity = {}
    by_status = {}
    total_amount = Decimal("0")

    for error in errors:
        type_key = error.error_type.value
        by_type[type_key] = by_type.get(type_key, 0) + 1

        severity_key = error.severity.value
        by_severity[severity_key] = by_severity.get(severity_key, 0) + 1

        status_key = error.status.value
        by_status[status_key] = by_status.get(status_key, 0) + 1

        if error.amount:
            total_amount += error.amount

    return {
        "total_errors": len(errors),
        "total_amount_affected": str(total_amount),
        "by_type": by_type,
        "by_severity": by_severity,
        "by_status": by_status,
        "resolved_count": sum(1 for e in errors if e.status in [ErrorStatus.CORRECTION_APPLIED, ErrorStatus.DISMISSED]),
        "pending_count": sum(1 for e in errors if e.status in [ErrorStatus.DETECTED, ErrorStatus.REVIEWING, ErrorStatus.PENDING_CORRECTION]),
    }


@app.get("/errors/pattern-detection")
async def detect_error_patterns():
    """Detect patterns in accounting errors"""
    errors = list(accounting_errors.values())
    patterns = []

    # Detect repeated errors on same account
    account_errors = {}
    for error in errors:
        if error.account_number:
            if error.account_number not in account_errors:
                account_errors[error.account_number] = []
            account_errors[error.account_number].append(error)

    for account, errs in account_errors.items():
        if len(errs) >= 3:
            patterns.append({
                "pattern_type": "repeated_errors",
                "account": account,
                "error_count": len(errs),
                "description": f"Account {account} has {len(errs)} errors - requires review",
                "recommendation": "Investigate account structure and user training",
            })

    # Detect period-related errors
    period_errors = {}
    for error in errors:
        if error.entry_date:
            period = error.entry_date.strftime("%Y-%m")
            if period not in period_errors:
                period_errors[period] = []
            period_errors[period].append(error)

    for period, errs in period_errors.items():
        if len(errs) >= 5:
            patterns.append({
                "pattern_type": "period_concentration",
                "period": period,
                "error_count": len(errs),
                "description": f"Period {period} has {len(errs)} errors - possibly system issue",
                "recommendation": "Review period-end closing procedures",
            })

    return {
        "patterns_detected": len(patterns),
        "patterns": patterns,
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8096)