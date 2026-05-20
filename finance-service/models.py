from pydantic import BaseModel, Field, condecimal, model_validator
from typing import Optional, List
from datetime import datetime
from decimal import Decimal

class BudgetItemBase(BaseModel):
    category: str = Field(..., example="Rent", description="Category for the budget item.")
    budgeted_amount: condecimal(ge=Decimal('0.00'), decimal_places=2) = Field(..., description="Budgeted amount for this item.")
    actual_amount: condecimal(ge=Decimal('0.00'), decimal_places=2) = Field(Decimal('0.00'), description="Actual amount spent/earned for this item.")
    description: Optional[str] = Field(None, example="Monthly office rent.", description="Description of the budget item.")
    account_number: Optional[str] = Field(None, example="6000", description="Associated accounting account number (from Accounting Service).")
    period_start: datetime = Field(..., description="Start date of the budget item period.")
    period_end: datetime = Field(..., description="End date of the budget item period.")

class BudgetItemCreate(BudgetItemBase):
    pass

class BudgetItemInDB(BudgetItemBase):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

class BudgetBase(BaseModel):
    name: str = Field(..., example="Q2 2026 Operating Budget", description="Name of the budget.")
    fiscal_year: int = Field(..., example=2026, description="Fiscal year the budget belongs to.")
    period: str = Field(..., example="Q2", description="Budget period (e.g., 'Q1', 'Month 5', 'Annual').")
    description: Optional[str] = Field(None, example="Operating budget for the second quarter of 2026.", description="Description of the budget.")
    status: str = Field("Draft", example="Approved", description="Current status of the budget (Draft, Approved, Closed).")
    items: List[BudgetItemCreate] = Field(..., description="List of budget items.")

    @model_validator(mode='after')
    def check_budget_dates(self) -> 'BudgetBase':
        for item in self.items:
            if item.period_start >= item.period_end:
                raise ValueError("Budget item period_start must be before period_end.")
        return self

class BudgetCreate(BudgetBase):
    pass

class BudgetUpdate(BaseModel):
    name: Optional[str] = None
    fiscal_year: Optional[int] = None
    period: Optional[str] = None
    description: Optional[str] = None
    status: Optional[str] = None
    items: Optional[List[BudgetItemCreate]] = None # Update items is complex, typically involves item-level CRUD

class BudgetInDB(BudgetBase):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    
    items: List[BudgetItemInDB] # Items returned will have their own IDs

    class Config:
        from_attributes = True
