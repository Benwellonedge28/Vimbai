# ... (existing Budget and BudgetItem models) ...

# --- Variance Analysis Models (NEW) ---
class ActualsSummary(BaseModel):
    account_number: str
    total_debit: Decimal
    total_credit: Decimal
    balance: Decimal # Balance based on normal_balance

class BudgetVarianceItem(BaseModel):
    category: str
    account_number: Optional[str]
    budgeted_amount: Decimal
    actual_amount: Decimal
    variance: Decimal # Actual - Budgeted
    variance_percentage: float # (Variance / Budgeted) * 100

class BudgetVarianceReport(BaseModel):
    budget_name: str
    fiscal_year: int
    period: str
    report_date: datetime = Field(default_factory=datetime.utcnow)
    items: List[BudgetVarianceItem]
    total_budgeted: Decimal
    total_actual: Decimal
    total_variance: Decimal
    total_variance_percentage: float # (Total Variance / Total Budgeted) * 100
