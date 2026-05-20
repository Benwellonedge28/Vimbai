# ... (existing models) ...

# Cash Flow Statement (Expanded)
class CashFlowActivity(BaseModel):
    description: str = Field(..., description="Description of the cash flow item.")
    amount: condecimal(ge=Decimal('-999999999999999.99'), decimal_places=2) = Field(..., description="Cash flow amount.")

class CashFlowSection(BaseModel):
    title: str = Field(..., description="Title of the cash flow section (e.g., Operating Activities).")
    activities: List[CashFlowActivity] = Field(..., description="List of individual cash flow activities.")
    net_cash: condecimal(ge=Decimal('-999999999999999.99'), decimal_places=2) = Field(..., description="Net cash flow for this section.")

class CashFlowStatement(BaseModel):
    report_date: datetime = Field(default_factory=datetime.utcnow, description="Date the cash flow statement was generated.")
    start_date: datetime = Field(..., description="Start date of the reporting period.")
    end_date: datetime = Field(..., description="End date of the reporting period.")
    net_income: condecimal(ge=Decimal('-999999999999999.99'), decimal_places=2) = Field(..., description="Net Income from Income Statement.")
    operating_activities: CashFlowSection = Field(..., description="Cash flow from operating activities.")
    investing_activities: CashFlowSection = Field(..., description="Cash flow from investing activities.")
    financing_activities: CashFlowSection = Field(..., description="Cash flow from financing activities.")
    net_increase_decrease_in_cash: condecimal(ge=Decimal('-999999999999999.99'), decimal_places=2) = Field(..., description="Net increase/decrease in cash during the period.")
    beginning_cash_balance: condecimal(ge=Decimal('-999999999999999.99'), decimal_places=2) = Field(..., description="Cash balance at the beginning of the period.")
    ending_cash_balance: condecimal(ge=Decimal('-999999999999999.99'), decimal_places=2) = Field(..., description="Cash balance at the end of the period.")
