"""Pydantic models for Org Authorization Engine (API contract unchanged)."""

import uuid
from datetime import datetime, timezone
from typing import List

from pydantic import BaseModel, Field


class Role(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    permissions: List[str] = []
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class UserAssignment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: str
    org_id: str
    role_id: str
    assigned_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    assigned_by: str = ""


class Permission(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    resource: str
    action: str  # read, write, delete, admin


class CheckRequest(BaseModel):
    user_id: str
    org_id: str
    permission: str
    resource: str = ""
