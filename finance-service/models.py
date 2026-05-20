from pydantic import BaseModel, Field, condecimal, validator # NEW: validator
from typing import Optional, List, Literal
from datetime import datetime
from decimal import Decimal

# ... (existing Budget and Variance Analysis models) ...

# --- Financial Ratio Models (EXPANDED) ---

class LiquidityRatios(BaseModel):
    current_ratio: Optional[Decimal] = Field(None, description="Current Assets / Current Liabilities")
    quick_ratio: Optional[Decimal] = Field(None, description="(Cash + Marketable Securities + Accounts Receivable) / Current Liabilities")
    cash_ratio: Optional[Decimal] = Field(None, description="(Cash + Marketable Securities) / Current Liabilities")

class SolvencyRatios(BaseModel):
    debt_to_equity_ratio: Optional[Decimal] = Field(None, description="Total Liabilities / Total Equity")
    debt_to_asset_ratio: Optional[Decimal] = Field(None, description="Total Liabilities / Total Assets")
    equity_multiplier: Optional[Decimal] = Field(None, description="Total Assets / Total Equity")
    interest_coverage_ratio: Optional[Decimal] = Field(None, description="EBIT / Interest Expense (EBIT assumed as Net Income + Interest Expense + Tax Expense for simplicity)")

class ProfitabilityRatios(BaseModel):
    gross_profit_margin: Optional[Decimal] = Field(None, description="(Revenue - Cost of Goods Sold) / Revenue")
    operating_profit_margin: Optional[Decimal] = Field(None, description="Operating Income / Revenue") # Operating Income assumed
    net_profit_margin: Optional[Decimal] = Field(None, description="Net Income / Revenue")
    return_on_assets: Optional[Decimal] = Field(None, description="Net Income / Average Total Assets (Average Total Assets approximated as current Total Assets)")
    return_on_equity: Optional[Decimal] = Field(None, description="Net Income / Average Total Equity (Average Total Equity approximated as current Total Equity)")
    earnings_per_share: Optional[Decimal] = Field(None, description="Net Income / Shares Outstanding (Requires shares outstanding, often external data)")
    return_on_capital_employed: Optional[Decimal] = Field(None, description="EBIT / Capital Employed (Capital Employed = Total Assets - Current Liabilities)")
    
class EfficiencyRatios(BaseModel):
    inventory_turnover: Optional[Decimal] = Field(None, description="Cost of Goods Sold / Average Inventory (Requires COGS and Average Inventory, approximated as current Inventory)")
    accounts_receivable_turnover: Optional[Decimal] = Field(None, description="Credit Sales / Average Accounts Receivable (Credit Sales approximated as Total Revenue, Average AR approximated as current AR)")
    accounts_payable_turnover: Optional[Decimal] = Field(None, description="Cost of Goods Sold / Average Accounts Payable (Requires COGS and Average AP, approximated as current AP)")
    asset_turnover: Optional[Decimal] = Field(None, description="Total Revenue / Average Total Assets (Average Total Assets approximated as current Total Assets)")
    days_sales_outstanding: Optional[Decimal] = Field(None, description="365 / Accounts Receivable Turnover")
    days_inventory_outstanding: Optional[Decimal] = Field(None, description="365 / Inventory Turnover")

class MarketRatios(BaseModel):
    # These typically require external market data (e.g., share price)
    price_earnings_ratio: Optional[Decimal] = Field(None, description="Share Price / EPS (Requires external share price data)")
    dividend_yield: Optional[Decimal] = Field(None, description="Annual Dividend Per Share / Share Price (Requires external share price and dividend data)")

class FinancialRatiosReport(BaseModel):
    report_date: datetime = Field(default_factory=datetime.utcnow)
    start_date: datetime
    end_date: datetime
    liquidity: LiquidityRatios = Field(default_factory=LiquidityRatios) # Ensure defaults are initialized
    solvency: SolvencyRatios = Field(default_factory=SolvencyRatios)
    profitability: ProfitabilityRatios = Field(default_factory=ProfitabilityRatios)
    efficiency: EfficiencyRatios = Field(default_factory=EfficiencyRatios) # NEW
    market: MarketRatios = Field(default_factory=MarketRatios) # NEW
    currency: str = Field("USD", description="Currency of the financial data.")

# --- Error Response Model (unchanged) ---
class ErrorResponse(BaseModel):
    detail: str
    code: Optional[str] = None
    status_code: int = 500
