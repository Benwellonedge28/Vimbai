"""Pydantic models for Message Bus Service (API contract unchanged)."""

import hashlib
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field

EXCHANGE_NAME = "vimbai_events"
DEAD_LETTER_EXCHANGE = "vimbai_dlx"


class EventType(str, Enum):
    # Accounting Events
    JOURNAL_ENTRY_CREATED = "accounting.journal_entry.created"
    JOURNAL_ENTRY_UPDATED = "accounting.journal_entry.updated"
    JOURNAL_ENTRY_POSTED = "accounting.journal_entry.posted"
    ACCOUNT_CREATED = "accounting.account.created"
    ACCOUNT_UPDATED = "accounting.account.updated"
    TRIAL_BALANCE_GENERATED = "accounting.trial_balance.generated"

    # Finance Events
    BUDGET_CREATED = "finance.budget.created"
    BUDGET_UPDATED = "finance.budget.updated"
    BUDGET_VARIANCE_ALERT = "finance.budget.variance_alert"
    SCENARIO_CREATED = "finance.scenario.created"

    # Transaction Events
    TRANSACTION_CREATED = "banking.transaction.created"
    TRANSACTION_RECONCILED = "banking.transaction.reconciled"
    TRANSACTION_FLAGGED = "fraud.transaction.flagged"

    # Workflow Events
    APPROVAL_REQUESTED = "workflow.approval.requested"
    APPROVAL_COMPLETED = "workflow.approval.completed"
    APPROVAL_REJECTED = "workflow.approval.rejected"

    # Integration Events
    POS_SYNC_COMPLETED = "integration.pos.sync_completed"
    BANK_FEED_RECEIVED = "integration.bank_feed.received"
    INVENTORY_UPDATED = "integration.inventory.updated"

    # Multimodal Events
    DOCUMENT_PROCESSED = "multimodal.document.processed"
    VOICE_TRANSCRIPT_COMPLETE = "multimodal.voice.transcript_complete"

    # System Events
    SERVICE_HEALTHY = "system.service.healthy"
    SERVICE_UNHEALTHY = "system.service.unhealthy"
    FEATURE_TOGGLED = "system.feature.toggled"
    DATA_SYNC_COMPLETED = "system.data_sync.completed"


class EventPriority(str, Enum):
    LOW = "low"
    NORMAL = "normal"
    HIGH = "high"
    CRITICAL = "critical"


class Event(BaseModel):
    id: str = Field(default_factory=lambda: hashlib.md5(str(datetime.utcnow()).encode()).hexdigest()[:16])
    type: EventType
    source_service: str
    timestamp: datetime = Field(default_factory=datetime.utcnow)
    priority: EventPriority = EventPriority.NORMAL
    payload: Dict[str, Any]
    correlation_id: Optional[str] = None
    reply_to: Optional[str] = None
    headers: Optional[Dict[str, str]] = None
    retry_count: int = 0
    max_retries: int = 3


class EventSubscription(BaseModel):
    id: str
    name: str
    event_types: List[EventType]
    callback_url: str
    filter_expression: Optional[str] = None
    enabled: bool = True
    priority: EventPriority = EventPriority.NORMAL


class QueueConfig(BaseModel):
    name: str
    durable: bool = True
    auto_delete: bool = False
    max_length: Optional[int] = None
    message_ttl: Optional[int] = None
    dead_letter_exchange: str = DEAD_LETTER_EXCHANGE
