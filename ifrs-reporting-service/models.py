"""Pydantic models for the IFRS Reporting Service.

:IfrsReport via :OWNS_IFRS_REPORT and :DisclosureNote via :OWNS_DISCLOSURE_NOTE,
both Book-gated. The report payload and the disclosure index are kept as JSON
strings on the node.
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class IFRSReport(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    report_type: str  # balance_sheet, income_statement, cash_flow, equity_changes, notes
    ifrs_standard: str  # IAS1, IFRS16, IFRS15, IFRS9, IAS36, etc.
    period: str  # YYYY-MM or YYYY
    reporting_date: datetime
    data: Dict[str, Any] = {}
    disclosures: List[Dict[str, Any]] = []
    status: str = "draft"  # draft, reviewed, approved, published
    created_at: datetime = Field(default_factory=_utcnow)
    approved_by: str = ""


class DisclosureNote(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    report_id: str
    note_number: int
    title: str
    content: str = ""
    ifrs_reference: str = ""
