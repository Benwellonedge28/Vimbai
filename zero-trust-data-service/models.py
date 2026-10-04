"""Pydantic models for the Zero Trust Data Service."""

import uuid
from datetime import datetime, timezone
from typing import List

from pydantic import BaseModel, Field


class AccessPolicy(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    resource: str
    required_roles: List[str] = []
    required_clearance: str = "standard"  # public, internal, confidential, restricted
    mfa_required: bool = True
    ip_whitelist: List[str] = []
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class AccessAttempt(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: str
    resource: str
    policy_id: str
    user_roles: List[str] = []
    user_clearance: str = "standard"
    mfa_verified: bool = False
    source_ip: str = ""
    granted: bool = False
    timestamp: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    reason: str = ""


class EvaluateRequest(BaseModel):
    user_id: str
    resource: str
    user_roles: List[str] = []
    user_clearance: str = "standard"
    mfa_verified: bool = False
    source_ip: str = ""
