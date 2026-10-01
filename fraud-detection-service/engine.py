"""Fraud detection engine: pure rule evaluation, no storage."""

from typing import Any, Dict, List, Optional

from fraud_detection_service.models import FraudAlert, FraudRule, RiskLevel, Transaction


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
