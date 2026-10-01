"""Pydantic models for the Supply Chain Service (gateway-registered contract)."""

import uuid
from datetime import datetime, timezone
from typing import List, Optional

from pydantic import BaseModel, Field


class Supplier(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    contact: str = ""
    lead_time_days: int = 7
    rating: float = 5.0
    products: List[str] = []


class InventoryItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    sku: str
    name: str
    company_id: str
    quantity: int = 0
    reorder_point: int = 10
    reorder_qty: int = 50
    unit_cost: float = 0
    unit_price: float = 0
    supplier_id: Optional[str] = None
    lead_time_days: int = 7


class PurchaseOrder(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    supplier_id: str
    item_sku: str
    quantity: int
    unit_cost: float
    status: str = "pending"  # pending, approved, shipped, received
    order_date: str = Field(default_factory=lambda: datetime.now(timezone.utc).isoformat())
    expected_delivery: Optional[str] = None


class DemandForecast(BaseModel):
    sku: str
    company_id: str
    historical_data: List[float] = []  # units sold per period
    forecast_periods: int = 3


class ForecastResult(BaseModel):
    sku: str
    forecast: List[float]
    method: str
    confidence: float
    reorder_recommended: bool
    recommended_qty: int = 0
