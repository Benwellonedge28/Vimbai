"""Pydantic models and enums for POS Integration Service (API contract unchanged)."""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


class POSDeviceStatus(str, Enum):
    ONLINE = "online"
    OFFLINE = "offline"
    SYNCING = "syncing"
    ERROR = "error"


class TransactionType(str, Enum):
    SALE = "sale"
    REFUND = "refund"
    VOID = "void"
    ADJUSTMENT = "adjustment"
    LAYAWAY = "layaway"
    RETURN = "return"


class PaymentMethod(str, Enum):
    CASH = "cash"
    CARD = "card"
    MOBILE = "mobile"
    SPLIT = "split"
    GIFT_CARD = "gift_card"
    LOYALTY = "loyalty"


class SyncStatus(str, Enum):
    PENDING = "pending"
    SYNCED = "synced"
    FAILED = "failed"
    PARTIAL = "partial"


class POSDeviceCreate(BaseModel):
    device_id: str = Field(..., description="Unique POS device identifier")
    device_name: str = Field(..., min_length=3, max_length=100)
    device_type: str = Field(..., description="POS hardware type")
    location_id: Optional[str] = None
    api_key: Optional[str] = None
    webhook_url: Optional[str] = None
    enabled: bool = True


class POSDeviceInDB(POSDeviceCreate):
    id: str
    status: POSDeviceStatus = POSDeviceStatus.OFFLINE
    last_sync: Optional[datetime] = None
    created_at: datetime
    updated_at: datetime


class POSTransactionCreate(BaseModel):
    transaction_id: str = Field(..., description="External POS transaction ID")
    device_id: str
    transaction_type: TransactionType
    total_amount: float = Field(..., gt=0)
    tax_amount: float = 0
    discount_amount: float = 0
    payment_method: PaymentMethod
    payment_details: Optional[Dict[str, Any]] = None
    # min-items relaxed from 1: the webhook transforms (Square/Stripe/Shopify)
    # carry no line items, and the original min_items=1 made every webhook
    # ingest fail validation (latent bug — webhooks could never succeed).
    items: List[Dict[str, Any]] = Field(...)
    customer_id: Optional[str] = None
    employee_id: Optional[str] = None
    location_id: Optional[str] = None
    notes: Optional[str] = None
    timestamp: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class POSTransactionInDB(POSTransactionCreate):
    id: str
    sync_status: SyncStatus = SyncStatus.PENDING
    journal_entry_id: Optional[str] = None
    processed_at: Optional[datetime] = None
    error_message: Optional[str] = None
    created_at: datetime


class InventorySyncRequest(BaseModel):
    device_id: str
    products: List[Dict[str, Any]] = Field(..., description="Product inventory updates")
    timestamp: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class SalesSummaryRequest(BaseModel):
    device_id: str
    start_date: datetime
    end_date: datetime
    group_by: str = "hour"  # hour, day, week
