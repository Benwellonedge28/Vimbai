from pydantic import BaseModel, Field, condecimal, validator # NEW: validator for BudgetItem
from typing import Optional, List, Literal, Dict, Any
from datetime import datetime
from decimal import Decimal

# --- Budget Item Models (refining validation) ---
class BudgetItemBase(BaseModel):
    category: str = Field(..., min_length=3, max_length=100, description="Category of the budget item (e.g., 'Salaries', 'Rent', 'Marketing').")
    account_number: str = Field(..., min_length=4, max_length=10, regex=r"^\d+$", description="Associated accounting account number.")
    budgeted_amount: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(..., description="Budgeted amount for this item.")
    budget_type: Literal['expense', 'revenue'] = Field(..., description="Whether this is an expense or revenue budget item.")

    @validator('budgeted_amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

class BudgetItemCreate(BudgetItemBase):
    pass

class BudgetItemUpdate(BaseModel):
    category: Optional[str] = Field(None, min_length=3, max_length=100)
    account_number: Optional[str] = Field(None, min_length=4, max_length=10, regex=r"^\d+$")
    budgeted_amount: Optional[condecimal(decimal_places=2, ge=Decimal('0.00'))] = None
    budget_type: Optional[Literal['expense', 'revenue']] = None

    @validator('budgeted_amount', pre=True)
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
    name: str = Field(..., min_length=3, max_length=100, description="Name of the budget (e.g., 'Q1 2026 Marketing Budget').")
    start_date: datetime = Field(..., description="Start date of the budget period.")
    end_date: datetime = Field(..., description="End date of the budget period.")
    currency: str = Field("USD", max_length=5, description="Currency of the budget.")
    description: Optional[str] = Field(None, max_length=500, description="Description of the budget.")
    # Removed items from here, they will be linked via relationship in Neo4j

    @validator('end_date')
    def validate_end_date(cls, v, values):
        if 'start_date' in values and v < values['start_date']:
            raise ValueError('End date cannot be before start date.')
        return v

class BudgetCreate(BudgetBase):
    pass # Budget items will be added later

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
    items: List[BudgetItemInDB] = [] # List of linked budget items

    class Config:
        from_attributes = True

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
