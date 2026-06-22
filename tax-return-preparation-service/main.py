"""
Tax Return Preparation Service
Port: 8228
Tax computation and return preparation
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Tax Return Preparation Service", version="1.0.0")

class TaxComponent(BaseModel):
    component_name: str
    taxable_amount: float
    tax_rate: float
    tax_amount: float

class TaxReturnRequest(BaseModel):
    company_id: str
    fiscal_year: str
    jurisdictions: List[str]
    accounting_profit: float
    permanent_differences: List[Dict[str, Any]]
    temporary_differences: List[Dict[str, Any]]
    tax_credits: List[Dict[str, Any]]

class TaxReturnResponse(BaseModel):
    company_id: str
    fiscal_year: str
    tax_components: List[TaxComponent]
    total_taxable_income: float
    total_tax_payable: float
    effective_tax_rate: float
    deferred_tax_assets: float
    deferred_tax_liabilities: float
    tax_credits_applied: float
    return_status: str
    recommendations: List[str]

async def call_internal_service(service_url: str, endpoint: str, data: dict = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            response = await client.post(url, json=data) if data else await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "tax-return-preparation", "version": "1.0.0"}

@app.post("/prepare", response_model=TaxReturnResponse)
async def prepare_tax_return(request: TaxReturnRequest):
    logger.info("Preparing tax return", company=request.company_id, year=request.fiscal_year)

    taxable_income = request.accounting_profit

    for perm in request.permanent_differences:
        taxable_income += perm.get("adjustment", 0)

    deferred_tax_assets = sum(td.get("dtl_assets", 0) for td in request.temporary_differences)
    deferred_tax_liabilities = sum(td.get("dtl_liabilities", 0) for td in request.temporary_differences)

    tax_rate = 0.25
    total_tax = taxable_income * tax_rate

    tax_credits = sum(tc.get("credit_amount", 0) for tc in request.tax_credits)
    net_tax = max(0, total_tax - tax_credits)

    effective_rate = net_tax / request.accounting_profit if request.accounting_profit else 0

    tax_components = [
        TaxComponent(component_name="Corporate Income Tax", taxable_amount=taxable_income, tax_rate=tax_rate, tax_amount=total_tax),
        TaxComponent(component_name="Tax Credits", taxable_amount=0, tax_rate=0, tax_amount=-tax_credits)
    ]

    return TaxReturnResponse(
        company_id=request.company_id,
        fiscal_year=request.fiscal_year,
        tax_components=tax_components,
        total_taxable_income=round(taxable_income, 2),
        total_tax_payable=round(net_tax, 2),
        effective_tax_rate=round(effective_rate, 4),
        deferred_tax_assets=round(deferred_tax_assets, 2),
        deferred_tax_liabilities=round(deferred_tax_liabilities, 2),
        tax_credits_applied=round(tax_credits, 2),
        return_status="ready_for_filing",
        recommendations=["Review all tax positions", "Ensure documentation is complete", "File before deadline"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8228)
