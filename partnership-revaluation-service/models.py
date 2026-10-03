"""Pydantic models for Partnership Revaluation Service (API contract unchanged)."""

import uuid
from datetime import datetime
from enum import Enum
from typing import Dict, List, Optional

from pydantic import BaseModel, Field


class GoodwillTreatment(str, Enum):
    RAISE_AND_RAISE = "raise_and_raise"
    WRITE_OFF_AGAINST_RESERVES = "write_off_against_reserves"
    ELIMINATE_FROM_BOOKS = "eliminate_from_books"


class RevaluationEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_id: str
    asset_name: str
    asset_code: str
    old_value: float
    new_value: float
    increase: float = 0
    decrease: float = 0
    revaluation_gain: float = 0
    revaluation_loss: float = 0
    journal_entry_id: Optional[str] = None


class RevaluationReport(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    partnership_id: str
    revaluation_date: datetime
    entries: List[RevaluationEntry] = []
    total_increase: float = 0
    total_decrease: float = 0
    net_gain: float = 0
    goodwill_amount: float = 0
    goodwill_treatment: GoodwillTreatment
    new_profit_sharing_ratios: Dict[str, float] = {}
    journal_entry_ids: List[str] = []
    created_at: datetime = Field(default_factory=datetime.utcnow)
