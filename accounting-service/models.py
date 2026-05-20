# ... (existing imports and models) ...

# --- Financial Statement Models (NEW) ---

# Income Statement
class IncomeStatementItem(BaseModel):
    category: str = Field(..., description="Revenue or Expense category.")
    amount: condecimal(ge=Decimal('-999999999999999.99'), decimal_places=2) = Field(..., description="Net amount for the category.")

class IncomeStatement(BaseModel):
    report_date: datetime = Field(default_factory=datetime.utcnow, description="Date the income statement was generated.")
    start_date: datetime = Field(..., description="Start date of the reporting period.")
    end_date: datetime = Field(..., description="End date of the reporting period.")
    revenues: List[IncomeStatementItem] = Field(..., description="List of revenue items.")
    expenses: List[IncomeStatementItem] = Field(..., description="List of expense items.")
    net_income: condecimal(ge=Decimal('-999999999999999.99'), decimal_places=2) = Field(..., description="Calculated net income.")

# Balance Sheet
class BalanceSheetItem(BaseModel):
    category: str = Field(..., description="Asset, Liability, or Equity category.")
    amount: condecimal(ge=Decimal('-999999999999999.99'), decimal_places=2) = Field(..., description="Total amount for the category.")

class BalanceSheet(BaseModel):
    report_date: datetime = Field(default_factory=datetime.utcnow, description="Date the balance sheet was generated.")
    as_of_date: datetime = Field(..., description="The specific date for which the balance sheet is prepared.")
    assets: List[BalanceSheetItem] = Field(..., description="List of asset items.")
    liabilities: List[BalanceSheetItem] = Field(..., description="List of liability items.")
    equity: List[BalanceSheetItem] = Field(..., description="List of equity items.")
    total_assets: condecimal(ge=Decimal('0.00'), decimal_places=2) = Field(..., description="Sum of all assets.")
    total_liabilities_equity: condecimal(ge=Decimal('0.00'), decimal_places=2) = Field(..., description="Sum of all liabilities and equity.")

    @model_validator(mode='after')
    def check_balanced_balance_sheet(self) -> 'BalanceSheet':
        if self.total_assets != self.total_liabilities_equity:
            raise ValueError("Balance Sheet is not balanced: Total Assets must equal Total Liabilities + Equity.")
        return self

# Cash Flow Statement (Placeholder for future)
class CashFlowStatement(BaseModel):
    report_date: datetime = Field(default_factory=datetime.utcnow)
    start_date: datetime = Field(...)
    end_date: datetime = Field(...)
    operating_activities: List[dict] = []
    investing_activities: List[dict] = []
    financing_activities: List[dict] = []
    net_cash_flow: condecimal(ge=Decimal('-999999999999999.99'), decimal_places=2) = Field(Decimal('0.00'))
