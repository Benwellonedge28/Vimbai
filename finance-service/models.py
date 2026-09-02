from datetime import date, datetime  # NEW: Import date for scenario parameters
from decimal import Decimal
from typing import Any, Dict, List, Literal, Optional

from pydantic import BaseModel, Field, condecimal, validator


# --- Budget Item Models (refining validation) ---
class BudgetItemBase(BaseModel):
    category: str = Field(
        ...,
        min_length=3,
        max_length=100,
        description="Category of the budget item (e.g., 'Salaries', 'Rent', 'Marketing').",
    )
    account_number: str = Field(
        ..., min_length=4, max_length=10, regex=r"^\\d+$", description="Associated accounting account number."
    )
    budgeted_amount: condecimal(decimal_places=2, ge=Decimal("0.00")) = Field(
        ..., description="Budgeted amount for this item."
    )
    budget_type: Literal["expense", "revenue"] = Field(
        ..., description="Whether this is an expense or revenue budget item."
    )

    @validator("budgeted_amount", pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v


class BudgetItemCreate(BudgetItemBase):
    pass


class BudgetItemUpdate(BaseModel):
    category: Optional[str] = Field(None, min_length=3, max_length=100)
    account_number: Optional[str] = Field(None, min_length=4, max_length=10, regex=r"^\\d+$")
    budgeted_amount: Optional[condecimal(decimal_places=2, ge=Decimal("0.00"))] = None
    budget_type: Optional[Literal["expense", "revenue"]] = None

    @validator("budgeted_amount", pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v


class BudgetItemInDB(BudgetItemBase):
    id: str = Field(..., example="uuid-string-for-node")
    budget_id: str = Field(..., description="ID of the parent Budget.")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


# --- Budget Models (unchanged, as items will be managed separately) ---
class BudgetBase(BaseModel):
    name: str = Field(
        ..., min_length=3, max_length=100, description="Name of the budget (e.g., 'Q1 2026 Marketing Budget')."
    )
    start_date: datetime = Field(..., description="Start date of the budget period.")
    end_date: datetime = Field(..., description="End date of the budget period.")
    currency: str = Field("USD", max_length=5, description="Currency of the budget.")
    description: Optional[str] = Field(None, max_length=500, description="Description of the budget.")
    # Removed items from here, they will be linked via relationship in Neo4j

    @validator("end_date")
    def validate_end_date(cls, v, values):
        if "start_date" in values and v < values["start_date"]:
            raise ValueError("End date cannot be before start date.")
        return v


class BudgetCreate(BudgetBase):
    pass  # Budget items will be added later


class BudgetUpdate(BudgetBase):
    name: Optional[str] = None
    start_date: Optional[datetime] = None
    end_date: Optional[datetime] = None
    currency: Optional[str] = None
    description: Optional[str] = None


class BudgetInDB(BudgetBase):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    items: List[BudgetItemInDB] = []  # List of linked budget items

    class Config:
        from_attributes = True


# --- Financial Forecast Models (NEW) ---
class FinancialForecastDataPoint(BaseModel):
    period: date = Field(..., description="Date representing the forecast period (e.g., start of month).")
    value_type: str = Field(..., description="Type of value (e.g., 'revenue', 'expenses', 'profit').")
    amount: condecimal(max_digits=18, decimal_places=2) = Field(..., description="Forecasted amount for the period.")

    @validator("amount", pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v


class FinancialForecastBase(BaseModel):
    name: str = Field(..., min_length=3, max_length=100, description="Name of the financial forecast.")
    description: Optional[str] = Field(None, max_length=500, description="Description of the forecast.")
    start_date: date = Field(..., description="Start date of the forecast period.")
    end_date: date = Field(..., description="End date of the forecast period.")
    forecast_type: Literal["revenue", "expenses", "profit", "cash_flow", "custom"] = Field(
        ..., description="Type of financial metric being forecasted."
    )
    methodology: Optional[str] = Field(
        None, description="Methodology used for forecasting (e.g., 'historical_average', 'regression', 'AI_model')."
    )
    owner_user_id: str = Field(..., description="User ID who created/owns this forecast.")
    data_points: List[FinancialForecastDataPoint] = Field([], description="List of forecasted data points.")

    @validator("end_date")
    def validate_forecast_end_date(cls, v, values):
        if "start_date" in values and v < values["start_date"]:
            raise ValueError("Forecast end date cannot be before start date.")
        return v


class FinancialForecastCreate(FinancialForecastBase):
    pass


class FinancialForecastUpdate(BaseModel):
    name: Optional[str] = None
    description: Optional[str] = None
    start_date: Optional[date] = None
    end_date: Optional[date] = None
    forecast_type: Optional[Literal["revenue", "expenses", "profit", "cash_flow", "custom"]] = None
    methodology: Optional[str] = None
    data_points: Optional[List[FinancialForecastDataPoint]] = None


class FinancialForecastInDB(FinancialForecastBase):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


# --- Scenario Models (NEW) ---
class ScenarioParameter(BaseModel):
    name: str = Field(..., description="Name of the parameter (e.g., 'Sales Growth Rate').")
    value: float = Field(..., description="Value of the parameter (e.g., 0.10 for 10% growth).")
    param_type: Literal["percentage", "absolute_amount", "multiplier"] = Field(
        ..., description="Type of the parameter value."
    )
    target_metric: str = Field(..., description="Metric this parameter influences (e.g., 'revenue', 'cogs').")
    description: Optional[str] = Field(None, description="Description of the parameter.")


class ScenarioBase(BaseModel):
    name: str = Field(
        ...,
        min_length=3,
        max_length=100,
        description="Name of the scenario (e.g., 'Best Case Growth', 'Economic Downturn').",
    )
    description: Optional[str] = Field(None, max_length=500, description="Description of the scenario.")
    base_forecast_id: Optional[str] = Field(None, description="ID of the FinancialForecast this scenario is based on.")
    parameters: List[ScenarioParameter] = Field([], description="List of parameters to adjust for this scenario.")
    owner_user_id: str = Field(..., description="User ID who created/owns this scenario.")


class ScenarioCreate(ScenarioBase):
    pass


class ScenarioUpdate(BaseModel):
    name: Optional[str] = None
    description: Optional[str] = None
    base_forecast_id: Optional[str] = None
    parameters: Optional[List[ScenarioParameter]] = None


class ScenarioInDB(ScenarioBase):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True


# --- Scenario Analysis Result (NEW) ---
class ScenarioAnalysisResult(BaseModel):
    scenario_id: str = Field(..., description="ID of the Scenario analyzed.")
    forecast_id: Optional[str] = Field(None, description="ID of the FinancialForecast used as base (if applicable).")
    analysis_date: datetime = Field(default_factory=datetime.utcnow)
    summary: str = Field(..., description="Summary of the analysis results.")
    projected_metrics: Dict[str, Any] = Field(
        {}, description="Key projected financial metrics (e.g., 'total_revenue': 12345.67)."
    )
    projected_data_points: List[FinancialForecastDataPoint] = Field(
        [], description="The detailed forecast generated by the scenario."
    )


# --- Actuals, Variance, Financial Ratios (unchanged) ---
class ActualsSummary(BaseModel):
    # ... (unchanged) ...
    pass


class BudgetVarianceItem(BaseModel):
    # ... (unchanged) ...
    pass


class BudgetVarianceReport(BaseModel):
    # ... (unchanged) ...
    pass


class LiquidityRatios(BaseModel):
    # ... (unchanged) ...
    pass


class SolvencyRatios(BaseModel):
    # ... (unchanged) ...
    pass


class ProfitabilityRatios(BaseModel):
    # ... (unchanged) ...
    pass


class EfficiencyRatios(BaseModel):
    # ... (unchanged) ...
    pass


class MarketRatios(BaseModel):
    # ... (unchanged) ...
    pass


class FinancialRatiosReport(BaseModel):
    # ... (unchanged) ...
    pass


class ErrorResponse(BaseModel):
    # ... (unchanged) ...
    pass
