from pydantic import BaseModel, Field, condecimal, validator
from typing import Optional, List, Literal
from datetime import datetime
from decimal import Decimal

# ... (existing Budget and Variance Analysis models) ...

# --- Financial Ratio Models (EXPANDED) ---

class LiquidityRatios(BaseModel):
    current_ratio: Optional[Decimal] = None   # Current Assets / Current Liabilities
    quick_ratio: Optional[Decimal] = None     # (Cash + Marketable Securities + Accounts Receivable) / Current Liabilities
    cash_ratio: Optional[Decimal] = None      # Cash / Current Liabilities
    working_capital: Optional[Decimal] = None # Current Assets - Current Liabilities

class SolvencyRatios(BaseModel):
    debt_to_equity_ratio: Optional[Decimal] = None  # Total Liabilities / Shareholder's Equity
    debt_to_asset_ratio: Optional[Decimal] = None   # Total Liabilities / Total Assets
    equity_multiplier: Optional[Decimal] = None     # Total Assets / Shareholder's Equity
    times_interest_earned: Optional[Decimal] = None # EBIT / Interest Expense (requires more detailed IS)

class ProfitabilityRatios(BaseModel):
    gross_profit_margin: Optional[Decimal] = None # (Revenue - Cost of Goods Sold) / Revenue
    operating_profit_margin: Optional[Decimal] = None # Operating Income (EBIT) / Revenue
    net_profit_margin: Optional[Decimal] = None   # Net Income / Revenue
    return_on_assets: Optional[Decimal] = None    # Net Income / Total Assets
    return_on_equity: Optional[Decimal] = None    # Net Income / Shareholder's Equity

class EfficiencyRatios(BaseModel): # NEW CATEGORY
    inventory_turnover: Optional[Decimal] = None      # Cost of Goods Sold / Average Inventory
    accounts_receivable_turnover: Optional[Decimal] = None # Sales Revenue / Average Accounts Receivable
    accounts_payable_turnover: Optional[Decimal] = None    # Cost of Goods Sold / Average Accounts Payable
    asset_turnover: Optional[Decimal] = None              # Sales Revenue / Average Total Assets
    day_sales_outstanding: Optional[Decimal] = None       # (Average Accounts Receivable / Sales Revenue) * 365

class MarketValueRatios(BaseModel): # NEW CATEGORY (Conceptual for a private company, needs shares data)
    earnings_per_share: Optional[Decimal] = None # Net Income / Number of Shares Outstanding
    price_to_earnings_ratio: Optional[Decimal] = None # Share Price / Earnings Per Share
    book_value_per_share: Optional[Decimal] = None # (Total Equity - Preferred Stock) / Number of Shares Outstanding

class FinancialRatiosReport(BaseModel):
    report_date: datetime = Field(default_factory=datetime.utcnow)
    start_date: datetime
    end_date: datetime
    liquidity: LiquidityRatios = Field(default_factory=LiquidityRatios) # Ensure default instances
    solvency: SolvencyRatios = Field(default_factory=SolvencyRatios)
    profitability: ProfitabilityRatios = Field(default_factory=ProfitabilityRatios)
    efficiency: EfficiencyRatios = Field(default_factory=EfficiencyRatios) # NEW
    market_value: MarketValueRatios = Field(default_factory=MarketValueRatios) # NEW
    currency: str = Field("USD", description="Currency of the financial data.")
