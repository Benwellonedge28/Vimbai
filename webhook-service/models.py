"""Pydantic models for the Webhook Service."""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


class WebhookEndpoint(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    url: str
    secret: str = ""
    events: List[str] = []  # e.g., ["invoice.created", "payment.received"]
    active: bool = True
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class WebhookEndpointCreate(BaseModel):
    company_id: str
    url: str
    secret: str = ""
    events: List[str] = []
    active: bool = True


class WebhookDelivery(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    endpoint_id: str
    event_type: str
    payload: Dict[str, Any]
    status: str = "pending"  # pending, delivered, failed
    attempts: int = 0
    response_code: int = 0
    last_attempt: Optional[datetime] = None
