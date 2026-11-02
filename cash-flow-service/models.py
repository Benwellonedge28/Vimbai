"""Pydantic models for the Cash Flow Service.

A schedule is a project's yearly inflows and outflows, kept as one node with the
years held in a JSON prop. `initial_investment` and the totals ride on the node
so a schedule reads back exactly as it was written.
"""

import uuid
from datetime import datetime, timezone
from typing import List, Optional

from pydantic import BaseModel, Field


class CashFlowEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    year: int
    inflow: float = 0
    outflow: float = 0
    net_flow: float = 0
    description: str = ""


class CashFlowSummary(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    project_name: str
    initial_investment: float
    entries: List[CashFlowEntry] = []
    total_inflows: float = 0
    total_outflows: float = 0
    total_net_flow: float = 0
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
