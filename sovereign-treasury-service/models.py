"""Pydantic models for the Sovereign Treasury Service."""

import uuid
from datetime import datetime, timezone
from typing import Optional

from pydantic import BaseModel, Field


class SovereignAccount(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    country: str
    account_type: str  # consolidated_revenue, stabilization_fund, debt_management, foreign_reserves
    balance: float
    currency: str = "USD"
    description: str = ""


class SovereignDebt(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    country: str
    instrument: str  # treasury_bill, government_bond, external_debt, eurobond
    principal: float
    interest_rate: float
    maturity_date: datetime
    outstanding: float
    currency: str = "USD"


class FiscalPosition(BaseModel):
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    country: str
    fiscal_year: str
    total_revenue: float
    total_expenditure: float
    fiscal_deficit: float = 0
    deficit_to_gdp: float = 0
    total_debt: float = 0
    debt_to_gdp: float = 0
    foreign_reserves: float = 0
    months_import_cover: float = 0
