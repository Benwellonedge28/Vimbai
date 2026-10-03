"""Pydantic models for Partnership Dissolution Service (API contract unchanged)."""

import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List

from pydantic import BaseModel, Field


class DissolutionReason(str, Enum):
    MUTUAL_AGREEMENT = "mutual_agreement"
    EXPIRY_OF_TERM = "expiry_of_term"
    COMPLETION_OF_ADVENTURE = "completion_of_adventure"
    DEATH_OF_PARTNER = "death_of_partner"
    INSOLVENCY = "insolvency"
    COURT_ORDER = "court_order"


class AssetRealization(BaseModel):
    asset_id: str
    asset_name: str
    book_value: float
    sale_proceeds: float
    profit: float = 0
    loss: float = 0


class CreditorSettlement(BaseModel):
    creditor_id: str
    creditor_name: str
    amount_owed: float
    amount_paid: float
    discount_received: float = 0


class PartnerSettlement(BaseModel):
    partner_id: str
    partner_name: str
    capital_balance: float
    current_account_balance: float
    share_of_profit_loss: float
    total_due: float


class DissolutionReport(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    partnership_id: str
    dissolution_date: datetime
    reason: DissolutionReason
    total_assets_realized: float = 0
    total_liabilities_paid: float = 0
    total_creditors: float = 0
    total_partners_capitals: float = 0
    realization_profit: float = 0
    realization_loss: float = 0
    assets: List[AssetRealization] = []
    creditors: List[CreditorSettlement] = []
    partners: List[PartnerSettlement] = []
    journal_entry_ids: List[str] = []
    status: str = "pending"
    created_at: datetime = Field(default_factory=datetime.utcnow)
