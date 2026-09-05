"""Pydantic models for the Cash Flow Statement Service."""

import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, Field


class CashFlowMethod(str, Enum):
    DIRECT = "direct"
    INDIRECT = "indirect"


class CashFlowLine(BaseModel):
    description: str
    amount: float
    is_inflow: bool = True


class CashFlowStatementCreate(BaseModel):
    """Payload for generating a statement (totals computed server side)."""

    company_id: str
    method: CashFlowMethod = CashFlowMethod.DIRECT
    period_start: Optional[datetime] = None
    period_end: Optional[datetime] = None
    beginning_cash: float = 0
    operating_activities: List[CashFlowLine] = []
    investing_activities: List[CashFlowLine] = []
    financing_activities: List[CashFlowLine] = []


class CashFlowStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    company_id: str
    method: CashFlowMethod = CashFlowMethod.DIRECT
    period_start: Optional[datetime] = None
    period_end: Optional[datetime] = None
    operating_activities: List[CashFlowLine] = []
    investing_activities: List[CashFlowLine] = []
    financing_activities: List[CashFlowLine] = []
    net_operating: float = 0
    net_investing: float = 0
    net_financing: float = 0
    net_change: float = 0
    beginning_cash: float = 0
    ending_cash: float = 0
    created_at: Optional[datetime] = None


def calc_net(lines: List[CashFlowLine]) -> float:
    return sum(line.amount if line.is_inflow else -line.amount for line in lines)


def compute_totals(stmt: CashFlowStatement) -> None:
    """Fill net_* / ending_cash in place (server-side computation)."""
    stmt.net_operating = calc_net(stmt.operating_activities)
    stmt.net_investing = calc_net(stmt.investing_activities)
    stmt.net_financing = calc_net(stmt.financing_activities)
    stmt.net_change = stmt.net_operating + stmt.net_investing + stmt.net_financing
    stmt.ending_cash = stmt.beginning_cash + stmt.net_change
