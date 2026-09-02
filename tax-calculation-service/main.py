"""
Vimbai Tax Calculation Service
Computes income tax, VAT, payroll tax, withholding tax, and capital gains.
Port: 8335
"""
import os, uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "tax-calculation-service"
PORT = int(os.getenv("PORT", "8335"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Tax Calculation Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class TaxType(str, Enum):
    INCOME_TAX = "income_tax"; VAT = "vat"; PAYE = "paye"; CAPITAL_GAINS = "capital_gains"; WITHHOLDING = "withholding"

class TaxBracket(BaseModel):
    min_amount: float; max_amount: Optional[float]; rate: float

class TaxCalculationRequest(BaseModel):
    company_id: str; tax_type: TaxType; taxable_amount: float; fiscal_year: int = 2026
    country: str = "ZW"; deductions: float = 0; brackets: Optional[List[TaxBracket]] = None

class TaxResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; tax_type: TaxType; taxable_amount: float; deductions: float
    tax_owed: float; effective_rate: float; marginal_rate: float
    breakdown: Dict[str, float] = {}; fiscal_year: int; calculated_at: str = Field(default_factory=lambda: datetime.now(timezone.utc).isoformat())

# Zimbabwe tax brackets 2026 (example)
ZW_BRACKETS = [
    TaxBracket(min_amount=0, max_amount=12500, rate=0.0),
    TaxBracket(min_amount=12500, max_amount=25000, rate=0.20),
    TaxBracket(min_amount=25000, max_amount=75000, rate=0.25),
    TaxBracket(min_amount=75000, max_amount=None, rate=0.35),
]

def calculate_progressive_tax(amount: float, brackets: List[TaxBracket]) -> tuple:
    tax_owed = 0.0; breakdown = {}; marginal_rate = 0.0
    remaining = max(amount, 0)
    for bracket in brackets:
        if remaining <= 0: break
        bracket_max = bracket.max_amount if bracket.max_amount else float("inf")
        taxable_in_bracket = min(remaining, bracket_max - bracket.min_amount)
        if taxable_in_bracket > 0:
            bracket_tax = taxable_in_bracket * bracket.rate
            tax_owed += bracket_tax
            breakdown[f"bracket_{bracket.min_amount}_{bracket.rate}"] = bracket_tax
            marginal_rate = bracket.rate
            remaining -= taxable_in_bracket
    return tax_owed, breakdown, marginal_rate

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/calculate", response_model=TaxResult)
async def calculate_tax(req: TaxCalculationRequest):
    brackets = req.brackets or ZW_BRACKETS
    net_taxable = max(req.taxable_amount - req.deductions, 0)
    
    if req.tax_type == TaxType.VAT:
        tax_owed = net_taxable * 0.15  # ZW VAT rate
        return TaxResult(company_id=req.company_id, tax_type=req.tax_type, taxable_amount=req.taxable_amount,
                         deductions=req.deductions, tax_owed=round(tax_owed, 2),
                         effective_rate=0.15, marginal_rate=0.15,
                         breakdown={"vat_standard": tax_owed}, fiscal_year=req.fiscal_year)
    elif req.tax_type == TaxType.WITHHOLDING:
        rate = 0.10 if req.country == "ZW" else 0.15
        tax_owed = net_taxable * rate
        return TaxResult(company_id=req.company_id, tax_type=req.tax_type, taxable_amount=req.taxable_amount,
                         deductions=req.deductions, tax_owed=round(tax_owed, 2),
                         effective_rate=rate, marginal_rate=rate,
                         breakdown={"withholding": tax_owed}, fiscal_year=req.fiscal_year)
    
    tax_owed, breakdown, marginal_rate = calculate_progressive_tax(net_taxable, brackets)
    effective_rate = tax_owed / req.taxable_amount if req.taxable_amount > 0 else 0
    return TaxResult(company_id=req.company_id, tax_type=req.tax_type, taxable_amount=req.taxable_amount,
                     deductions=req.deductions, tax_owed=round(tax_owed, 2),
                     effective_rate=round(effective_rate, 4), marginal_rate=marginal_rate,
                     breakdown=breakdown, fiscal_year=req.fiscal_year)

@app.post("/calculate/capital-gains", response_model=TaxResult)
async def calculate_capital_gains(company_id: str, asset_name: str, purchase_price: float, sale_price: float, holding_period_days: int, fiscal_year: int = 2026):
    gain = max(sale_price - purchase_price, 0)
    is_long_term = holding_period_days > 365
    rate = 0.20 if is_long_term else 0.35
    tax_owed = gain * rate
    return TaxResult(company_id=company_id, tax_type=TaxType.CAPITAL_GAINS, taxable_amount=sale_price,
                     deductions=purchase_price, tax_owed=round(tax_owed, 2),
                     effective_rate=rate, marginal_rate=rate,
                     breakdown={"gain": gain, "rate": rate, "long_term": is_long_term}, fiscal_year=fiscal_year)

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
