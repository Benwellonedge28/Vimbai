"""Pydantic models for the Basis Apportionment Service.

:ApportionmentRule via :OWNS_APPORTIONMENT_RULE and :ApportionmentResult via
:OWNS_APPORTIONMENT_RESULT, both Book-gated. Each result's per-centre breakdown is
kept as a JSON string, since Neo4j has no list-of-maps property.
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class ApportionmentRule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    cost_type: str  # rent, rates, depreciation, insurance, maintenance, etc.
    basis: str  # floor_area, volume, cost, etc.
    description: str
    created_at: datetime = Field(default_factory=_utcnow)


class ApportionmentResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    cost_type: str
    total_cost: float
    basis: str
    cost_centres: List[Dict[str, Any]] = []
    total_apportioned: float = 0
    unapportioned: float = 0
    created_at: datetime = Field(default_factory=_utcnow)
