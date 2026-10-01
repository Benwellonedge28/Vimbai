"""Pydantic models for the Policy Engine Service."""

import uuid
from enum import Enum
from typing import Any, Optional

from pydantic import BaseModel, Field


class PolicyAction(str, Enum):
    ALLOW = "allow"
    DENY = "deny"
    WARN = "warn"
    REQUIRE_APPROVAL = "require_approval"


class PolicyRule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    name: str
    description: str = ""
    resource_type: str  # transaction, invoice, payment, report
    condition_field: str  # e.g., "amount", "currency", "category"
    condition_operator: str = ">"  # >, <, ==, >=, <=, contains
    condition_value: Any
    action: PolicyAction = PolicyAction.WARN
    message: str = ""
    enabled: bool = True


class PolicyEvaluation(BaseModel):
    rule_id: str
    rule_name: str
    action: PolicyAction
    message: str
    triggered: bool = False
