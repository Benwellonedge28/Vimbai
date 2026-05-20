# ... (existing Budget and Variance Analysis models) ...

# --- Financial Ratio Models (NEW) ---

class LiquidityRatios(BaseModel):
    current_ratio: Optional[Decimal] = None # Current Assets / Current Liabilities
    quick_ratio: Optional[Decimal] = None   # (Cash + Marketable Securities + Accounts Receivable) / Current Liabilities

class SolvencyRatios(BaseModel):
    debt_to_equity_ratio: Optional[Decimal] = None # Total Debt / Shareholder's Equity
    debt_to_asset_ratio: Optional[Decimal] = None  # Total Debt / Total Assets

class ProfitabilityRatios(BaseModel):
    gross_profit_margin: Optional[Decimal] = None # (Revenue - Cost of Goods Sold) / Revenue
    net_profit_margin: Optional[Decimal] = None   # Net Income / Revenue
    return_on_assets: Optional[Decimal] = None    # Net Income / Total Assets

class FinancialRatiosReport(BaseModel):
    report_date: datetime = Field(default_factory=datetime.utcnow)
    start_date: datetime
    end_date: datetime
    liquidity: LiquidityRatios
    solvency: SolvencyRatios
    profitability: ProfitabilityRatios
    currency: str = Field("USD", description="Currency of the financial data.")
