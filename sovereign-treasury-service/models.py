"""Pydantic models for the Sovereign Treasury Service."""

import uuid
from datetime import datetime, timezone
from decimal import Decimal
from typing import Optional

from pydantic import BaseModel, Field


class SovereignAccount(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    country: str
    account_type: str  # consolidated_revenue, stabilization_fund, debt_management, foreign_reserves
    balance: Decimal
    currency: str = "USD"
    description: str = ""


class SovereignDebt(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    country: str
    instrument: str  # treasury_bill, government_bond, external_debt, eurobond
    principal: Decimal
    interest_rate: Decimal
    maturity_date: datetime
    outstanding: Decimal
    currency: str = "USD"


class FiscalPosition(BaseModel):
    user_id: Optional[str] = None
    book_id: Optional[str] = None
    country: str
    fiscal_year: str
    total_revenue: Decimal
    total_expenditure: Decimal
    fiscal_deficit: Decimal = Decimal("0")
    deficit_to_gdp: Decimal = Decimal("0")
    total_debt: Decimal = Decimal("0")
    debt_to_gdp: Decimal = Decimal("0")
    foreign_reserves: Decimal = Decimal("0")
    months_import_cover: Decimal = Decimal("0")
