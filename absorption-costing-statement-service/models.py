"""Pydantic models for the Absorption Costing Statement Service."""

import uuid
from datetime import datetime
from typing import List

from pydantic import BaseModel, Field


class StatementLineItem(BaseModel):
    description: str
    amount: float
    is_total: bool = False
    is_subtotal: bool = False
    indent_level: int = 0


class TradingAccountStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period_start: datetime
    period_end: datetime
    line_items: List[StatementLineItem] = []

    # Trading Account Section
    opening_stock: float = 0
    purchases: float = 0
    carriage_inwards: float = 0
    closing_stock: float = 0
    cost_of_goods_sold: float = 0
    gross_profit: float = 0

    # Profit & Loss Section
    sales_revenue: float = 0
    distribution_costs: float = 0
    administrative_expenses: float = 0
    other_expenses: float = 0
    net_profit: float = 0

    status: str = "draft"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ProductionCostStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    period: str

    # Production Costs
    direct_materials_opening: float = 0
    direct_materials_purchases: float = 0
    direct_materials_closing: float = 0
    direct_materials_used: float = 0

    direct_labor: float = 0
    direct_expenses: float = 0
    prime_cost: float = 0

    factory_overhead: float = 0
    work_in_progress_opening: float = 0
    work_in_progress_closing: float = 0
    production_cost: float = 0

    units_produced: int = 0
    cost_per_unit: float = 0

    created_at: datetime = Field(default_factory=datetime.utcnow)
