"""
NPO Service Models - Non-Profit Organization Accounting

Comprehensive models covering all 100 NPO accounting concepts:
1. Fund Accounting (General, Restricted, Endowment, Capital, Project)
2. Net Assets (With/Without Donor Restrictions)
3. Revenue and Income (Donations, Grants, Memberships, etc.)
4. Assets and Liabilities
5. Financial Statements
6. Budgeting and Cost Allocation
7. Compliance and Governance
8. Performance and Impact Measurement
"""

from datetime import date, datetime
from decimal import Decimal
from enum import Enum
from typing import Any, Dict, List, Literal, Optional

from pydantic import BaseModel, Field, field_validator

# =============================================================================
# ENUMS FOR NPO ACCOUNTING
# =============================================================================


class FundType(str, Enum):
    GENERAL = "general"
    RESTRICTED = "restricted"
    TEMPORARILY_RESTRICTED = "temporarily_restricted"
    PERMANENTLY_RESTRICTED = "permanently_restricted"
    ENDOWMENT = "endowment"
    CAPITAL = "capital"
    PROJECT = "project"
    BOARD_DESIGNATED = "board_designated"
    OPERATING = "operating"


class NetAssetType(str, Enum):
    WITHOUT_DONOR_RESTRICTIONS = "without_donor_restrictions"
    WITH_DONOR_RESTRICTIONS = "with_donor_restrictions"
    WITH_PERMANENT_RESTRICTIONS = "with_permanent_restrictions"
    WITH_TEMPORARY_RESTRICTIONS = "with_temporary_restrictions"


class RevenueType(str, Enum):
    DONATION = "donation"
    GRANT = "grant"
    MEMBERSHIP_FEE = "membership_fee"
    SUBSCRIPTION = "subscription"
    FUNDRAISING = "fundraising"
    SPONSORSHIP = "sponsorship"
    LEGACY_BEQUEST = "legacy_bequest"
    INVESTMENT = "investment"
    IN_KIND = "in_kind"
    PROGRAM_SERVICE = "program_service"


class GrantStatus(str, Enum):
    APPLICATION = "application"
    APPROVED = "approved"
    ACTIVE = "active"
    SUSPENDED = "suspended"
    COMPLETED = "completed"
    TERMINATED = "terminated"
    CLOSED = "closed"


class ProjectStatus(str, Enum):
    PLANNING = "planning"
    APPROVED = "approved"
    ACTIVE = "active"
    ON_HOLD = "on_hold"
    COMPLETED = "completed"
    CANCELLED = "cancelled"
    CLOSED = "closed"


class BudgetStatus(str, Enum):
    DRAFT = "draft"
    SUBMITTED = "submitted"
    APPROVED = "approved"
    REJECTED = "rejected"
    AMENDED = "amended"
    CLOSED = "closed"


class ComplianceStatus(str, Enum):
    PENDING = "pending"
    IN_REVIEW = "in_review"
    COMPLIANT = "compliant"
    NON_COMPLIANT = "non_compliant"
    WAIVED = "waived"
    EXPIRED = "expired"


class AuditType(str, Enum):
    EXTERNAL = "external"
    INTERNAL = "internal"
    REGULATORY = "regulatory"
    SPECIAL = "special"


class AssetStatus(str, Enum):
    ACTIVE = "active"
    DISPOSED = "disposed"
    DEPRECIATED = "depreciated"
    IMPAIRED = "impaired"


# =============================================================================
# FUND ACCOUNTING MODELS (1-15)
# =============================================================================


class FundBase(BaseModel):
    """Base model for NPO Funds"""

    fund_code: str = Field(..., max_length=50, description="Unique fund identifier")
    fund_name: str = Field(..., max_length=200, description="Name of the fund")
    fund_type: FundType = Field(..., description="Type of fund")
    description: Optional[str] = Field(None, max_length=500, description="Fund description")
    purpose: str = Field(..., max_length=500, description="Purpose of the fund")
    created_date: Optional[date] = Field(None, description="Date fund was created")


class FundCreate(FundBase):
    """Model for creating a fund"""

    initial_balance: Decimal = Field(default_factory=lambda: Decimal("0.00"), description="Initial fund balance")
    currency: str = Field(default="USD", max_length=3, description="Currency code")
    parent_fund_id: Optional[str] = Field(None, description="Parent fund for hierarchical funds")


class FundInDB(FundBase):
    """Fund as stored in database"""

    id: str = Field(..., description="Unique identifier")
    user_id: str = Field(..., description="Organization/user ID")
    current_balance: Decimal = Field(..., description="Current fund balance")
    total_contributions: Decimal = Field(default_factory=lambda: Decimal("0.00"), description="Total contributions")
    total_disbursements: Decimal = Field(default_factory=lambda: Decimal("0.00"), description="Total disbursements")
    currency: str = Field(default="USD", max_length=3)
    parent_fund_id: Optional[str] = Field(None)
    status: str = Field(default="active", description="Fund status")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


class FundRestrictionBase(BaseModel):
    """Base model for fund restrictions"""

    restriction_type: Literal["donor_imposed", "board_designated", "legal", "contractual"] = Field(...)
    description: str = Field(..., max_length=500, description="Description of restriction")
    start_date: Optional[date] = Field(None, description="When restriction starts")
    end_date: Optional[date] = Field(None, description="When restriction ends (for temporary)")
    is_permanent: bool = Field(False, description="Whether restriction is permanent")
    terms_conditions: Optional[str] = Field(None, description="Terms and conditions")


class FundRestrictionCreate(FundRestrictionBase):
    """Model for creating fund restriction"""

    pass


class FundRestrictionInDB(FundRestrictionBase):
    """Fund restriction as stored"""

    id: str = Field(...)
    fund_id: str = Field(..., description="Associated fund ID")
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


class FundTransactionBase(BaseModel):
    """Base model for fund transactions"""

    transaction_date: date = Field(..., description="Date of transaction")
    transaction_type: Literal[
        "contribution",
        "disbursement",
        "transfer_in",
        "transfer_out",
        "investment",
        "appreciation",
        "depreciation",
        "adjustment",
    ] = Field(...)
    amount: Decimal = Field(..., description="Transaction amount")
    description: str = Field(..., max_length=500, description="Transaction description")
    reference_number: Optional[str] = Field(None, max_length=100, description="Reference/Cheque number")
    category: Optional[str] = Field(None, max_length=100, description="Transaction category")
    project_id: Optional[str] = Field(None, description="Associated project")
    grant_id: Optional[str] = Field(None, description="Associated grant")
    donor_id: Optional[str] = Field(None, description="Associated donor")
    created_by: Optional[str] = Field(None, max_length=200, description="Created by")


class FundTransactionCreate(FundTransactionBase):
    """Model for creating fund transaction"""

    pass


class FundTransactionInDB(FundTransactionBase):
    """Fund transaction as stored"""

    id: str = Field(...)
    fund_id: str = Field(..., description="Associated fund ID")
    balance_after: Decimal = Field(..., description="Balance after transaction")
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


# =============================================================================
# NET ASSETS MODELS (16-25)
# =============================================================================


class NetAssetsBase(BaseModel):
    """Base model for Net Assets"""

    as_of_date: date = Field(..., description="Date of net assets calculation")
    period_start: date = Field(..., description="Start of period")
    period_end: date = Field(..., description="End of period")


class NetAssetsInDB(NetAssetsBase):
    """Net Assets as stored"""

    id: str = Field(...)
    user_id: str = Field(..., description="Organization ID")

    # Net Asset Categories
    net_assets_without_donor_restrictions: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    net_assets_with_donor_restrictions: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    net_assets_with_permanent_restrictions: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    net_assets_with_temporary_restrictions: Decimal = Field(default_factory=lambda: Decimal("0.00"))

    # Endowments
    endowment_net_assets: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    board_designated_net_assets: Decimal = Field(default_factory=lambda: Decimal("0.00"))

    # Totals
    total_net_assets: Decimal = Field(..., description="Total net assets")
    accumulated_surplus: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    accumulated_deficit: Decimal = Field(default_factory=lambda: Decimal("0.00"))

    # Changes
    beginning_net_assets: Decimal = Field(..., description="Opening net assets")
    net_assets_change: Decimal = Field(default_factory=lambda: Decimal("0.00"))

    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


class NetAssetsChangeBase(BaseModel):
    """Model for net assets changes"""

    change_date: date = Field(...)
    change_type: Literal[
        "contribution", "grant", "investment_return", "expenditure", "reclassification", "depreciation", "impairment"
    ] = Field(...)
    amount: Decimal = Field(...)
    description: str = Field(..., max_length=500)
    restriction_status: Literal["unrestricted", "temporarily_restricted", "permanently_restricted"] = Field(...)
    donor_id: Optional[str] = Field(None)
    fund_id: Optional[str] = Field(None)


# =============================================================================
# REVENUE AND INCOME MODELS (26-50)
# =============================================================================


class DonationBase(BaseModel):
    """Base model for donations"""

    donation_date: date = Field(...)
    amount: Decimal = Field(..., description="Donation amount")
    donor_id: str = Field(..., description="Donor identifier")
    fund_id: Optional[str] = Field(None, description="Target fund")
    donation_type: Literal["cash", "check", "wire", "stock", "property", "crypto"] = Field(...)
    payment_method: Optional[str] = Field(None, max_length=50)
    campaign: Optional[str] = Field(None, max_length=200, description="Fundraising campaign")
    appeal: Optional[str] = Field(None, max_length=200, description="Appeal source")
    is_anonymous: bool = Field(False, description="Whether donation is anonymous")
    acknowledgement_sent: bool = Field(False)
    tax_deductible: bool = Field(True)
    notes: Optional[str] = Field(None)


class DonationCreate(DonationBase):
    """Model for creating donation"""

    pass


class DonationInDB(DonationBase):
    """Donation as stored"""

    id: str = Field(...)
    user_id: str = Field(...)
    receipt_number: str = Field(..., max_length=100)
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


class GrantBase(BaseModel):
    """Base model for grants"""

    grant_name: str = Field(..., max_length=200)
    grantor_name: str = Field(..., max_length=200, description="Funding organization")
    grant_type: Literal["government", "foundation", "corporate", "individual", "ngo"] = Field(...)
    status: GrantStatus = Field(...)
    application_date: Optional[date] = Field(None)
    approval_date: Optional[date] = Field(None)
    start_date: Optional[date] = Field(None)
    end_date: Optional[date] = Field(None)
    amount_awarded: Decimal = Field(...)
    amount_received: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    amount_spent: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    currency: str = Field(default="USD", max_length=3)
    purpose: str = Field(..., max_length=500)
    restrictions: Optional[str] = Field(None, description="Grant restrictions")
    reporting_requirements: Optional[str] = Field(None)
    fund_id: str = Field(..., description="Associated fund")


class GrantCreate(GrantBase):
    """Model for creating grant"""

    pass


class GrantInDB(GrantBase):
    """Grant as stored"""

    id: str = Field(...)
    user_id: str = Field(...)
    grant_code: str = Field(..., max_length=50)
    next_report_due: Optional[date] = Field(None)
    performance_indicators: Optional[Dict[str, Any]] = Field(default_factory=dict)
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


class GrantDrawdownBase(BaseModel):
    """Model for grant drawdowns"""

    drawdown_date: date = Field(...)
    amount: Decimal = Field(...)
    description: str = Field(..., max_length=500)
    invoice_number: Optional[str] = Field(None, max_length=100)
    status: Literal["requested", "approved", "disbursed", "rejected"] = Field(default="requested")


class MembershipFeeBase(BaseModel):
    """Base model for membership fees"""

    member_id: str = Field(...)
    member_name: str = Field(..., max_length=200)
    membership_type: str = Field(..., max_length=100)
    fee_amount: Decimal = Field(...)
    payment_date: Optional[date] = Field(None)
    payment_method: Optional[str] = Field(None, max_length=50)
    billing_cycle: Literal["monthly", "quarterly", "annually", "lifetime"] = Field(default="annually")
    renewal_date: Optional[date] = Field(None)
    status: Literal["active", "expired", "cancelled", "suspended"] = Field(default="active")


class MembershipFeeCreate(MembershipFeeBase):
    """Model for creating membership fee"""

    pass


class InKindContributionBase(BaseModel):
    """Base model for in-kind contributions"""

    contribution_date: date = Field(...)
    donor_id: str = Field(...)
    description: str = Field(..., max_length=500, description="Description of goods/services")
    category: Literal["goods", "services", "equipment", "software", "space", "professional"] = Field(...)
    fair_value: Decimal = Field(..., description="Fair market value")
    condition: Optional[str] = Field(None, max_length=100)
    usage_restrictions: Optional[str] = Field(None)
    acknowledged: bool = Field(False)


class InKindContributionCreate(InKindContributionBase):
    """Model for creating in-kind contribution"""

    pass


class FundraisingEventBase(BaseModel):
    """Base model for fundraising events"""

    event_name: str = Field(..., max_length=200)
    event_type: Literal["gala", "auction", "run_walk", "dinner", "concert", "campaign", "other"] = Field(...)
    event_date: date = Field(...)
    goal_amount: Optional[Decimal] = Field(None)
    raised_amount: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    expenses: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    net_proceeds: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    attendance: Optional[int] = Field(None)
    fund_id: Optional[str] = Field(None)
    status: Literal["planned", "completed", "cancelled"] = Field(default="planned")


class InvestmentIncomeBase(BaseModel):
    """Base model for investment income"""

    income_date: date = Field(...)
    income_type: Literal["interest", "dividend", "capital_gain", "rental", "royalty"] = Field(...)
    amount: Decimal = Field(...)
    source: str = Field(..., max_length=200, description="Investment source")
    fund_id: Optional[str] = Field(None)
    is_unrestricted: bool = Field(True)
    notes: Optional[str] = Field(None)


# =============================================================================
# ASSET AND LIABILITY MODELS (51-75)
# =============================================================================


class NPOAssetBase(BaseModel):
    """Base model for NPO assets"""

    asset_name: str = Field(..., max_length=200)
    asset_type: Literal["current", "fixed", "intangible", "long_term"] = Field(...)
    category: str = Field(..., max_length=100, description="Asset category")
    acquisition_date: date = Field(...)
    acquisition_cost: Decimal = Field(...)
    current_value: Decimal = Field(...)
    useful_life_years: Optional[int] = Field(None)
    depreciation_method: Optional[Literal["straight_line", "declining_balance", "units_of_activity"]] = Field(None)
    salvage_value: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    location: Optional[str] = Field(None, max_length=200)
    responsible_person: Optional[str] = Field(None, max_length=200)
    status: AssetStatus = Field(default=AssetStatus.ACTIVE)
    fund_id: Optional[str] = Field(None)


class NPOAssetCreate(NPOAssetBase):
    """Model for creating NPO asset"""

    pass


class NPOAssetInDB(NPOAssetBase):
    """NPO asset as stored"""

    id: str = Field(...)
    user_id: str = Field(...)
    asset_code: str = Field(..., max_length=50)
    accumulated_depreciation: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    net_book_value: Decimal = Field(...)
    depreciation_schedule: Optional[Dict[str, Any]] = Field(default_factory=dict)
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


class DepreciationEntryBase(BaseModel):
    """Base model for depreciation entries"""

    entry_date: date = Field(...)
    asset_id: str = Field(...)
    depreciation_amount: Decimal = Field(...)
    accumulated_depreciation: Decimal = Field(...)
    journal_entry_id: Optional[str] = Field(None)
    notes: Optional[str] = Field(None)


class EndowmentAssetBase(BaseModel):
    """Base model for endowment assets"""

    endowment_name: str = Field(..., max_length=200)
    fund_id: str = Field(...)
    investment_type: Literal["stocks", "bonds", "real_estate", "cash", "alternative"] = Field(...)
    original_value: Decimal = Field(...)
    current_value: Decimal = Field(...)
    income_distribution: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    appreciation: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    spending_policy: str = Field(..., max_length=500, description="Policy for spending endowment income")
    is_permanent: bool = Field(True)
    donor_id: Optional[str] = Field(None)


class LiabilityBase(BaseModel):
    """Base model for NPO liabilities"""

    liability_name: str = Field(..., max_length=200)
    liability_type: Literal["current", "long_term"] = Field(...)
    category: str = Field(..., max_length=100)
    amount: Decimal = Field(...)
    due_date: Optional[date] = Field(None)
    creditor: Optional[str] = Field(None, max_length=200)
    interest_rate: Optional[Decimal] = Field(None)
    collateral: Optional[str] = Field(None)
    status: Literal["outstanding", "paid", "cancelled"] = Field(default="outstanding")
    notes: Optional[str] = Field(None)


class DeferredRevenueBase(BaseModel):
    """Base model for deferred revenue"""

    revenue_date: date = Field(...)
    amount: Decimal = Field(...)
    source: str = Field(..., max_length=200)
    description: str = Field(..., max_length=500)
    recognition_date: date = Field(..., description="When revenue will be recognized")
    recognized_amount: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    remaining_amount: Decimal = Field(...)
    grant_id: Optional[str] = Field(None)
    membership_id: Optional[str] = Field(None)


class AccruedExpenseBase(BaseModel):
    """Base model for accrued expenses"""

    expense_date: date = Field(...)
    description: str = Field(..., max_length=500)
    amount: Decimal = Field(...)
    category: str = Field(..., max_length=100)
    vendor_id: Optional[str] = Field(None)
    invoice_number: Optional[str] = Field(None, max_length=100)
    status: Literal["accrued", "paid", "reversed"] = Field(default="accrued")


# =============================================================================
# FINANCIAL STATEMENT MODELS (76-100)
# =============================================================================


class StatementOfFinancialPositionBase(BaseModel):
    """Statement of Financial Position (Balance Sheet equivalent for NPOs)"""

    as_of_date: date = Field(...)


class StatementOfFinancialPositionInDB(StatementOfFinancialPositionBase):
    """Statement of Financial Position as stored"""

    id: str = Field(...)
    user_id: str = Field(...)

    # Assets
    current_assets: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    fixed_assets: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    intangible_assets: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    endowment_assets: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    other_assets: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    total_assets: Decimal = Field(...)

    # Liabilities
    current_liabilities: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    long_term_liabilities: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    total_liabilities: Decimal = Field(...)

    # Net Assets
    net_assets_without_restrictions: Decimal = Field(...)
    net_assets_with_donor_restrictions: Decimal = Field(...)
    total_net_assets: Decimal = Field(...)

    total_liabilities_net_assets: Decimal = Field(...)  # Should equal total_assets

    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


class StatementOfActivitiesLineBase(BaseModel):
    """Line item for Statement of Activities"""

    classification: Literal["revenue", "expense", "gain", "loss", "reclassification"] = Field(...)
    category: str = Field(..., max_length=100)
    description: str = Field(..., max_length=200)
    without_donor_restrictions: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    with_donor_restrictions: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    total: Decimal = Field(...)


class StatementOfActivitiesBase(BaseModel):
    """Statement of Activities (Income Statement equivalent for NPOs)"""

    period_start: date = Field(...)
    period_end: date = Field(...)


class StatementOfActivitiesInDB(StatementOfActivitiesBase):
    """Statement of Activities as stored"""

    id: str = Field(...)
    user_id: str = Field(...)

    # Revenue
    contributions_without_restrictions: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    contributions_with_restrictions: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    total_contributions: Decimal = Field(...)

    program_service_revenue: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    membership_dues: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    fundraising_revenue: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    investment_income: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    other_revenue: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    total_revenue: Decimal = Field(...)

    # Expenses
    program_expenses: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    administrative_expenses: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    fundraising_expenses: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    total_expenses: Decimal = Field(...)

    # Changes
    change_in_net_assets: Decimal = Field(...)
    net_assets_beginning: Decimal = Field(...)
    net_assets_ending: Decimal = Field(...)

    lines: List[StatementOfActivitiesLineBase] = Field(default_factory=list)
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


class StatementOfCashFlowsBase(BaseModel):
    """Statement of Cash Flows for NPOs"""

    period_start: date = Field(...)
    period_end: date = Field(...)


class StatementOfCashFlowsInDB(StatementOfCashFlowsBase):
    """Statement of Cash Flows as stored"""

    id: str = Field(...)
    user_id: str = Field(...)

    # Operating Activities
    cash_from_donations: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    cash_from_grants: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    cash_from_programs: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    cash_from_investments: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    cash_paid_to_suppliers: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    cash_paid_to_employees: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    cash_paid_for_fundraising: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    other_operating_cash: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    net_cash_operating: Decimal = Field(...)

    # Investing Activities
    cash_from_asset_sales: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    cash_paid_for_assets: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    cash_paid_for_investments: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    net_cash_investing: Decimal = Field(...)

    # Financing Activities
    cash_from_borrowings: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    cash_paid_for_debt: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    net_cash_financing: Decimal = Field(...)

    net_change_cash: Decimal = Field(...)
    cash_beginning: Decimal = Field(...)
    cash_ending: Decimal = Field(...)

    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


class StatementOfChangesInNetAssetsInDB(BaseModel):
    """Statement of Changes in Net Assets"""

    id: str = Field(...)
    user_id: str = Field(...)
    period_start: date = Field(...)
    period_end: date = Field(...)

    # Without Donor Restrictions
    beginning_unrestricted: Decimal = Field(...)
    contributions_unrestricted: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    program_expenses: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    administrative_expenses: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    other_changes_unrestricted: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    ending_unrestricted: Decimal = Field(...)

    # With Donor Restrictions
    beginning_temporarily_restricted: Decimal = Field(...)
    contributions_temporarily_restricted: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    released_from_restrictions: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    ending_temporarily_restricted: Decimal = Field(...)

    beginning_permanently_restricted: Decimal = Field(...)
    contributions_permanently_restricted: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    ending_permanently_restricted: Decimal = Field(...)

    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


# =============================================================================
# BUDGET AND COST ALLOCATION MODELS
# =============================================================================


class BudgetBase(BaseModel):
    """Base model for NPO budgets"""

    budget_name: str = Field(..., max_length=200)
    fiscal_year: str = Field(..., max_length=10, description="Fiscal year (e.g., 2024)")
    period_start: date = Field(...)
    period_end: date = Field(...)
    status: BudgetStatus = Field(default=BudgetStatus.DRAFT)
    total_budget: Decimal = Field(...)
    fund_id: Optional[str] = Field(None)
    project_id: Optional[str] = Field(None)
    program_id: Optional[str] = Field(None)


class BudgetCreate(BudgetBase):
    """Model for creating budget"""

    pass


class BudgetInDB(BudgetBase):
    """Budget as stored"""

    id: str = Field(...)
    user_id: str = Field(...)
    budget_code: str = Field(..., max_length=50)
    total_allocated: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    total_spent: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    remaining_balance: Decimal = Field(...)
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


class BudgetLineBase(BaseModel):
    """Base model for budget line items"""

    line_description: str = Field(..., max_length=200)
    category: Literal["program", "administrative", "fundraising", "capital", "other"] = Field(...)
    budgeted_amount: Decimal = Field(...)
    allocated_amount: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    spent_amount: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    variance: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    variance_percent: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    cost_allocation_method: Optional[str] = Field(None)
    notes: Optional[str] = Field(None)


class BudgetLineCreate(BudgetLineBase):
    """Model for creating budget line"""

    pass


class BudgetLineInDB(BudgetLineBase):
    """Budget line as stored"""

    id: str = Field(...)
    budget_id: str = Field(...)
    line_number: int = Field(...)
    is_over_budget: bool = Field(False)

    class Config:
        from_attributes = True


class CostAllocationBase(BaseModel):
    """Base model for cost allocation"""

    allocation_date: date = Field(...)
    cost_category: Literal["direct", "indirect", "administrative", "shared"] = Field(...)
    amount: Decimal = Field(...)
    description: str = Field(..., max_length=500)
    program_id: Optional[str] = Field(None)
    project_id: Optional[str] = Field(None)
    fund_id: Optional[str] = Field(None)
    allocation_basis: str = Field(..., max_length=200, description="How cost is allocated")
    notes: Optional[str] = Field(None)


class CostCenterBase(BaseModel):
    """Base model for cost centers"""

    cost_center_name: str = Field(..., max_length=200)
    cost_center_code: str = Field(..., max_length=50)
    cost_center_type: Literal["program", "administrative", "fundraising", "project"] = Field(...)
    budget_amount: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    spent_amount: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    manager: Optional[str] = Field(None, max_length=200)
    status: Literal["active", "inactive", "closed"] = Field(default="active")


# =============================================================================
# PROJECT AND PROGRAM MODELS
# =============================================================================


class ProjectBase(BaseModel):
    """Base model for NPO projects"""

    project_name: str = Field(..., max_length=200)
    project_code: str = Field(..., max_length=50)
    description: str = Field(..., max_length=500)
    status: ProjectStatus = Field(default=ProjectStatus.PLANNING)
    start_date: Optional[date] = Field(None)
    end_date: Optional[date] = Field(None)
    total_budget: Decimal = Field(...)
    spent_amount: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    funding_source: Optional[str] = Field(None, max_length=200)
    fund_id: Optional[str] = Field(None)
    program_id: Optional[str] = Field(None)
    location: Optional[str] = Field(None, max_length=200)
    target_beneficiaries: Optional[int] = Field(None)
    actual_beneficiaries: Optional[int] = Field(None)


class ProjectCreate(ProjectBase):
    """Model for creating project"""

    pass


class ProjectInDB(ProjectBase):
    """Project as stored"""

    id: str = Field(...)
    user_id: str = Field(...)
    completion_percent: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    key_milestones: Optional[List[Dict[str, Any]]] = Field(default_factory=list)
    risks: Optional[List[str]] = Field(default_factory=list)
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


class ProgramBase(BaseModel):
    """Base model for programs"""

    program_name: str = Field(..., max_length=200)
    program_code: str = Field(..., max_length=50)
    description: str = Field(..., max_length=500)
    mission_alignment: str = Field(..., max_length=500, description="How program aligns with mission")
    budget_amount: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    spent_amount: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    director: Optional[str] = Field(None, max_length=200)
    start_date: Optional[date] = Field(None)
    status: Literal["active", "inactive", "discontinued"] = Field(default="active")


class ProgramCreate(ProgramBase):
    """Model for creating program"""

    pass


class ProgramInDB(ProgramBase):
    """Program as stored"""

    id: str = Field(...)
    user_id: str = Field(...)
    program_type: str = Field(..., max_length=100)
    beneficiaries_served: Optional[int] = Field(None)
    outcomes_achieved: Optional[List[str]] = Field(default_factory=list)
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


# =============================================================================
# DONOR MODELS
# =============================================================================


class DonorBase(BaseModel):
    """Base model for donors"""

    donor_name: str = Field(..., max_length=200)
    donor_type: Literal["individual", "corporate", "foundation", "government", "ngo"] = Field(...)
    email: Optional[str] = Field(None, max_length=200)
    phone: Optional[str] = Field(None, max_length=50)
    address: Optional[str] = Field(None, max_length=500)
    tax_id: Optional[str] = Field(None, max_length=50)
    first_donation_date: Optional[date] = Field(None)
    last_donation_date: Optional[date] = Field(None)
    lifetime_donations: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    notes: Optional[str] = Field(None)


class DonorCreate(DonorBase):
    """Model for creating donor"""

    pass


class DonorInDB(DonorBase):
    """Donor as stored"""

    id: str = Field(...)
    user_id: str = Field(...)
    donor_code: str = Field(..., max_length=50)
    communication_preferences: Optional[Dict[str, bool]] = Field(default_factory=dict)
    stewardship_tier: Optional[str] = Field(None, max_length=50)
    preferred_fund: Optional[str] = Field(None)
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


class DonorStewardshipBase(BaseModel):
    """Base model for donor stewardship"""

    donor_id: str = Field(...)
    action_date: date = Field(...)
    action_type: Literal["acknowledgement", "thank_you", "impact_report", "invitation", "visit", "recognition"] = Field(
        ...
    )
    description: str = Field(..., max_length=500)
    conducted_by: Optional[str] = Field(None, max_length=200)
    notes: Optional[str] = Field(None)


# =============================================================================
# COMPLIANCE AND GOVERNANCE MODELS
# =============================================================================


class InternalControlBase(BaseModel):
    """Base model for internal controls"""

    control_name: str = Field(..., max_length=200)
    control_type: Literal["preventive", "detective", "corrective"] = Field(...)
    category: Literal["authorization", "segregation", "custody", "reconciliation", "documentation"] = Field(...)
    description: str = Field(..., max_length=500)
    implemented_date: date = Field(...)
    responsible_person: str = Field(..., max_length=200)
    frequency: Literal["daily", "weekly", "monthly", "quarterly", "annually"] = Field(default="monthly")
    last_reviewed: Optional[date] = Field(None)
    status: Literal["active", "inactive", "modified"] = Field(default="active")
    notes: Optional[str] = Field(None)


class InternalControlCreate(InternalControlBase):
    """Model for creating internal control"""

    pass


class InternalControlInDB(InternalControlBase):
    """Internal control as stored"""

    id: str = Field(...)
    user_id: str = Field(...)
    effectiveness_rating: Optional[int] = Field(None, description="1-5 rating")
    deficiencies: Optional[str] = Field(None)
    remediation_date: Optional[date] = Field(None)
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


class AuditReportBase(BaseModel):
    """Base model for audit reports"""

    audit_type: AuditType = Field(...)
    audit_name: str = Field(..., max_length=200)
    audit_period_start: date = Field(...)
    audit_period_end: date = Field(...)
    auditor_name: str = Field(..., max_length=200)
    auditor_firm: Optional[str] = Field(None, max_length=200)
    start_date: date = Field(...)
    end_date: Optional[date] = Field(None)
    status: Literal["scheduled", "in_progress", "completed", "published"] = Field(default="scheduled")
    findings: Optional[List[Dict[str, Any]]] = Field(default_factory=list)
    recommendations: Optional[List[str]] = Field(default_factory=list)
    overall_opinion: Optional[str] = Field(None, max_length=500)
    compliance_status: ComplianceStatus = Field(default=ComplianceStatus.PENDING)


class AuditReportCreate(AuditReportBase):
    """Model for creating audit report"""

    pass


class AuditReportInDB(AuditReportBase):
    """Audit report as stored"""

    id: str = Field(...)
    user_id: str = Field(...)
    report_number: str = Field(..., max_length=50)
    issues_count: int = Field(0)
    significant_findings: int = Field(0)
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


class ComplianceCheckBase(BaseModel):
    """Base model for compliance checks"""

    check_name: str = Field(..., max_length=200)
    compliance_type: Literal["regulatory", "legal", "contractual", "internal_policy"] = Field(...)
    description: str = Field(..., max_length=500)
    due_date: date = Field(...)
    status: ComplianceStatus = Field(default=ComplianceStatus.PENDING)
    responsible_person: Optional[str] = Field(None, max_length=200)
    evidence: Optional[str] = Field(None, description="Evidence of compliance")
    findings: Optional[str] = Field(None)
    remediation_plan: Optional[str] = Field(None)


class RegulatoryFilingBase(BaseModel):
    """Base model for regulatory filings"""

    filing_name: str = Field(..., max_length=200)
    filing_type: Literal["annual", "quarterly", "periodic", "ad_hoc"] = Field(...)
    regulatory_body: str = Field(..., max_length=200)
    due_date: date = Field(...)
    filed_date: Optional[date] = Field(None)
    status: Literal["pending", "filed", "late", "waived", "exempt"] = Field(default="pending")
    acknowledgment_number: Optional[str] = Field(None, max_length=100)
    notes: Optional[str] = Field(None)


# =============================================================================
# PERFORMANCE AND IMPACT MODELS
# =============================================================================


class ProgramMetricBase(BaseModel):
    """Base model for program metrics"""

    metric_name: str = Field(..., max_length=200)
    program_id: str = Field(...)
    metric_type: Literal["output", "outcome", "impact"] = Field(...)
    measurement_unit: str = Field(..., max_length=50)
    target_value: Decimal = Field(...)
    actual_value: Decimal = Field(...)
    variance: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    calculation_date: date = Field(...)
    methodology: Optional[str] = Field(None, max_length=500)
    notes: Optional[str] = Field(None)


class ProgramMetricCreate(ProgramMetricBase):
    """Model for creating program metric"""

    pass


class ImpactMeasurementBase(BaseModel):
    """Base model for impact measurement"""

    measurement_name: str = Field(..., max_length=200)
    program_id: Optional[str] = Field(None)
    project_id: Optional[str] = Field(None)
    impact_area: str = Field(..., max_length=200, description="Area of impact (e.g., education, health)")
    measurement_date: date = Field(...)
    beneficiaries_count: int = Field(...)
    measurement_type: str = Field(..., max_length=100)
    baseline_value: Optional[Decimal] = Field(None)
    current_value: Decimal = Field(...)
    change_value: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    change_percent: Decimal = Field(default_factory=lambda: Decimal("0.00"))
    methodology: str = Field(..., max_length=500)


class ImpactMeasurementCreate(ImpactMeasurementBase):
    """Model for creating impact measurement"""

    pass


class SROIAnalysisBase(BaseModel):
    """Social Return on Investment Analysis"""

    analysis_name: str = Field(..., max_length=200)
    program_id: Optional[str] = Field(None)
    project_id: Optional[str] = Field(None)
    analysis_date: date = Field(...)
    analysis_period: str = Field(..., max_length=50)

    # Inputs
    total_investment: Decimal = Field(...)

    # Outputs
    outputs_measured: List[str] = Field(default_factory=list)

    # Outcomes
    outcomes_achieved: List[str] = Field(default_factory=list)
    outcome_values: List[Decimal] = Field(default_factory=list)

    # SROI Calculation
    total_outcome_value: Decimal = Field(...)
    sroi_ratio: Decimal = Field(..., description="Social return ratio (e.g., 3:1)")
    confidence_level: str = Field(..., max_length=50)
    methodology: str = Field(..., max_length=500)
    assumptions: Optional[str] = Field(None)


class VolunteerRecordBase(BaseModel):
    """Base model for volunteer records"""

    volunteer_name: str = Field(..., max_length=200)
    volunteer_id: str = Field(...)
    activity_date: date = Field(...)
    hours_contributed: Decimal = Field(...)
    activity_type: str = Field(..., max_length=100)
    program_id: Optional[str] = Field(None)
    project_id: Optional[str] = Field(None)
    supervisor: Optional[str] = Field(None, max_length=200)
    description: Optional[str] = Field(None, max_length=500)
    is_skilled: bool = Field(False)
    hourly_rate_value: Optional[Decimal] = Field(None, description="Fair market value of time")


class VolunteerRecordCreate(VolunteerRecordBase):
    """Model for creating volunteer record"""

    pass


class VolunteerRecordInDB(VolunteerRecordBase):
    """Volunteer record as stored"""

    id: str = Field(...)
    user_id: str = Field(...)
    value_of_service: Decimal = Field(...)
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


# =============================================================================
# DONOR AND STAKEHOLDER REPORT MODELS
# =============================================================================


class DonorReportBase(BaseModel):
    """Base model for donor reports"""

    report_name: str = Field(..., max_length=200)
    donor_id: str = Field(...)
    report_type: Literal["annual", "impact", "financial", "custom"] = Field(...)
    report_date: date = Field(...)
    period_start: date = Field(...)
    period_end: date = Field(...)
    include_donation_summary: bool = Field(True)
    include_impact_story: bool = Field(True)
    include_tax_receipt: bool = Field(False)
    status: Literal["draft", "generated", "sent", "received"] = Field(default="draft")


class BeneficiaryAccountabilityBase(BaseModel):
    """Base model for beneficiary accountability"""

    program_id: str = Field(...)
    report_date: date = Field(...)
    beneficiaries_served: int = Field(...)
    services_provided: str = Field(..., max_length=500)
    outcomes_achieved: str = Field(..., max_length=500)
    feedback_summary: Optional[str] = Field(None)
    improvement_plan: Optional[str] = Field(None)


class SustainabilityReportBase(BaseModel):
    """Base model for sustainability reports"""

    report_name: str = Field(..., max_length=200)
    report_date: date = Field(...)
    reporting_period: str = Field(..., max_length=50)

    financial_sustainability: Dict[str, Any] = Field(default_factory=dict)
    program_sustainability: Dict[str, Any] = Field(default_factory=dict)
    organizational_sustainability: Dict[str, Any] = Field(default_factory=dict)
    environmental_impact: Dict[str, Any] = Field(default_factory=dict)

    risk_factors: List[str] = Field(default_factory=list)
    opportunities: List[str] = Field(default_factory=list)


# =============================================================================
# ERROR RESPONSE MODEL
# =============================================================================


class ErrorResponse(BaseModel):
    """Standard error response"""

    detail: str
    code: Optional[str] = None
    status_code: int = 500
