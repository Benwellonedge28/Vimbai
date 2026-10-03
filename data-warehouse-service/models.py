"""Pydantic models for Data Warehouse Service (API contract unchanged)."""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


class DimensionTable(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    columns: List[Dict[str, str]] = []  # [{"name": "id", "type": "int"}, ...]
    row_count: int = 0
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class FactTable(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    dimensions: List[str] = []  # dimension table names
    measures: List[Dict[str, str]] = []  # [{"name": "amount", "type": "decimal", "agg": "sum"}]
    row_count: int = 0
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class AggregateQuery(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    fact_table: str
    group_by: List[str] = []
    measures: List[str] = []
    filters: Dict[str, Any] = {}
    results: List[Dict[str, Any]] = []
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ETLJob(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    source: str
    target: str
    status: str = "pending"
    rows_processed: int = 0
    started_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None
