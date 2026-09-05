"""Pydantic models for the Trade Finance Service."""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


class InstrumentType(str, Enum):
    LETTER_OF_CREDIT = "letter_of_credit"
    DOCUMENTARY_COLLECTION = "documentary_collection"
    BANK_GUARANTEE = "bank_guarantee"
    ADVANCE_PAYMENT = "advance_payment"
    FACTORING = "factoring"


class TradeInstrumentCreate(BaseModel):
    company_id: str
    instrument_type: InstrumentType
    counterparty: str
    amount: float
    currency: str = "USD"
    expiry_date: str = ""
    issuing_bank: str = ""
    confirming_bank: str = ""


class TradeInstrument(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    instrument_type: InstrumentType
    counterparty: str
    amount: float
    currency: str = "USD"
    issue_date: str = Field(default_factory=lambda: datetime.now(timezone.utc).strftime("%Y-%m-%d"))
    expiry_date: str = ""
    status: str = "issued"  # issued, presented, accepted, paid, expired
    issuing_bank: str = ""
    confirming_bank: str = ""


class InstrumentResult(BaseModel):
    id: str
    company_id: str
    instrument_type: str
    amount: float
    fee_estimate: float
    status: str
    risk_assessment: str
    documentation_required: List[str] = []
