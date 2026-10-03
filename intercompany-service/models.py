"""Pydantic models for Intercompany Service (API contract unchanged)."""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


class IntercompanyEntity(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    legal_entity_code: str
    tax_jurisdiction: str = ""
    currency: str = "USD"
    status: str = "active"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class IntercompanyTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    from_entity_id: str
    to_entity_id: str
    transaction_type: str  # loan, service_fee, royalty, sale, cost_allocation
    amount: float
    currency: str = "USD"
    description: str = ""
    transfer_price_basis: str = "cost_plus"  # cost_plus, market, negotiated
    transaction_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    status: str = "pending"  # pending, matched, eliminated
    matched_transaction_id: Optional[str] = None


class EliminationEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    pair_id: str  # links to the matched pair
    debit_entity_id: str
    credit_entity_id: str
    amount: float
    description: str = ""
    elimination_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
