"""Utility functions for NPO Service"""

import uuid
from datetime import datetime
from decimal import Decimal
from typing import Optional, List, Dict, Any
from enum import Enum

def generate_uuid() -> str:
    """Generate a unique identifier"""
    return str(uuid.uuid4())

def utc_now() -> datetime:
    """Get current UTC datetime"""
    return datetime.utcnow()

def decimal_to_float(value: Any) -> float:
    """Convert decimal to float for Neo4j storage"""
    if isinstance(value, Decimal):
        return float(value)
    return value

def float_to_decimal(value: Any) -> Decimal:
    """Convert float to decimal"""
    if isinstance(value, (int, float)):
        return Decimal(str(value))
    return value

class FundType(str, Enum):
    """Types of NPO funds"""
    GENERAL = "general"
    RESTRICTED = "restricted"
    TEMPORARILY_RESTRICTED = "temporarily_restricted"
    PERMANENTLY_RESTRICTED = "permanently_restricted"
    ENDOWMENT = "endowment"
    CAPITAL = "capital"
    PROJECT = "project"
    BOARD_DESIGNATED = "board_designated"

class NetAssetType(str, Enum):
    """Net asset classification"""
    WITHOUT_DONOR_RESTRICTIONS = "without_donor_restrictions"
    WITH_DONOR_RESTRICTIONS = "with_donor_restrictions"

class RevenueType(str, Enum):
    """Types of NPO revenue"""
    DONATION = "donation"
    GRANT = "grant"
    MEMBERSHIP_FEE = "membership_fee"
    SUBSCRIPTION = "subscription"
    FUNDRAISING = "fundraising"
    SPONSORSHIP = "sponsorship"
    LEGACY_BEQUEST = "legacy_bequest"
    INVESTMENT = "investment"
    IN_KIND = "in_kind"
    PROGRAM_SERVICE = "program_service"

class GrantStatus(str, Enum):
    """Grant lifecycle status"""
    APPLICATION = "application"
    APPROVED = "approved"
    ACTIVE = "active"
    COMPLETED = "completed"
    TERMINATED = "terminated"
    CLOSED = "closed"

class ProjectStatus(str, Enum):
    """Project status"""
    PLANNING = "planning"
    ACTIVE = "active"
    ON_HOLD = "on_hold"
    COMPLETED = "completed"
    CANCELLED = "cancelled"

class BudgetStatus(str, Enum):
    """Budget status"""
    DRAFT = "draft"
    SUBMITTED = "submitted"
    APPROVED = "approved"
    REJECTED = "rejected"
    AMENDED = "amended"

class ComplianceStatus(str, Enum):
    """Compliance check status"""
    PENDING = "pending"
    IN_REVIEW = "in_review"
    COMPLIANT = "compliant"
    NON_COMPLIANT = "non_compliant"
    WAIVED = "waived"

def calculate_net_assets(
    total_assets: Decimal,
    total_liabilities: Decimal
) -> Dict[str, Decimal]:
    """Calculate net assets by type"""
    return {
        "total_net_assets": total_assets - total_liabilities,
        "net_assets_with_donor_restrictions": Decimal('0.00'),
        "net_assets_without_donor_restrictions": total_assets - total_liabilities
    }

def validate_fund_restriction(
    fund_type: str,
    transaction_type: str,
    amount: Decimal
) -> bool:
    """Validate if transaction is allowed under fund restrictions"""
    if fund_type == FundType.PERMANENTLY_RESTRICTED.value:
        # Permanently restricted funds can only have investment returns
        return transaction_type in ["investment_income", "appreciation"]
    elif fund_type == FundType.TEMPORARILY_RESTRICTED.value:
        # Temporarily restricted funds can only be used for specified purpose
        return True  # Validation done at transaction level
    elif fund_type == FundType.GENERAL.value:
        # General funds can be used for any purpose
        return True
    return True

def format_currency(amount: Decimal, currency: str = "USD") -> str:
    """Format decimal as currency string"""
    return f"{currency} {amount:,.2f}"

def calculate_variance(budgeted: Decimal, actual: Decimal) -> Dict[str, Any]:
    """Calculate budget variance"""
    variance = actual - budgeted
    variance_percent = (variance / budgeted * 100) if budgeted != 0 else Decimal('0.00')
    return {
        "variance": variance,
        "variance_percent": variance_percent,
        "favorable": variance <= 0  # Favorable if under budget
    }