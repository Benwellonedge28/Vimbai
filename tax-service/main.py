"""
Tax Service
Port: 8346
Tax calculation, compliance, and reporting
"""

from datetime import date, datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Tax Service", version="1.0.0")


class TaxCalculationRequest(BaseModel):
    company_id: str
    jurisdiction: str
    tax_type: str
    taxable_income: float
    deductions: List[Dict[str, Any]]
    tax_credits: List[Dict[str, Any]]
    period_start: date
    period_end: date


class TaxCalculationResponse(BaseModel):
    company_id: str
    jurisdiction: str
    tax_type: str
    gross_income: float
    total_deductions: float
    taxable_income: float
    tax_rate: float
    gross_tax: float
    tax_credits: float
    net_tax: float
    effective_rate: float


class TaxProvisionRequest(BaseModel):
    company_id: str
    interim_periods: int
    expected_annual_rate: float
    period_income: List[float]
    permanent_differences: List[Dict[str, Any]]
    temporary_differences: List[Dict[str, Any]]


class TaxProvisionResponse(BaseModel):
    company_id: str
    provision_periods: List[Dict[str, Any]]
    total_provision: float
    deferred_tax_assets: float
    deferred_tax_liabilities: float
    effective_tax_rate: float
    reconciliation: List[Dict[str, Any]]


class TransferPricingRequest(BaseModel):
    company_id: str
    related_party_id: str
    transaction_type: str
    transaction_amount: float
    currency: str
    comparable_data: List[Dict[str, Any]]


class TransferPricingResponse(BaseModel):
    company_id: str
    transaction_id: str
    arm_length_amount: float
    adjustment_required: float
    method_used: str
    range_low: float
    range_high: float
    compliance_status: str


class TaxComplianceRequest(BaseModel):
    company_id: str
    jurisdiction: str
    filing_type: str
    period: str
    amounts: Dict[str, float]


class TaxComplianceResponse(BaseModel):
    company_id: str
    filing_id: str
    status: str
    filed_date: datetime
    amounts_confirmed: Dict[str, float]
    penalties_interest: float
    next_filing_date: date


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "tax", "version": "1.0.0"}


@app.post("/calculate", response_model=TaxCalculationResponse)
async def calculate_tax(request: TaxCalculationRequest):
    logger.info(
        "Calculating tax", company=request.company_id, jurisdiction=request.jurisdiction, tax_type=request.tax_type
    )

    total_deductions = sum(d.get("amount", 0) for d in request.deductions)
    taxable = max(0, request.taxable_income - total_deductions)

    rates = {"US_FEDERAL": 0.21, "US_STATE": 0.05, "UK": 0.25, "EU": 0.23}
    rate = rates.get(request.jurisdiction, 0.25)
    gross_tax = taxable * rate

    tax_credits = sum(c.get("amount", 0) for c in request.tax_credits)
    net_tax = max(0, gross_tax - tax_credits)
    effective_rate = (net_tax / request.taxable_income * 100) if request.taxable_income else 0

    return TaxCalculationResponse(
        company_id=request.company_id,
        jurisdiction=request.jurisdiction,
        tax_type=request.tax_type,
        gross_income=request.taxable_income + total_deductions,
        total_deductions=round(total_deductions, 2),
        taxable_income=round(taxable, 2),
        tax_rate=rate,
        gross_tax=round(gross_tax, 2),
        tax_credits=round(tax_credits, 2),
        net_tax=round(net_tax, 2),
        effective_rate=round(effective_rate, 2),
    )


@app.post("/provision", response_model=TaxProvisionResponse)
async def calculate_tax_provision(request: TaxProvisionRequest):
    logger.info("Calculating tax provision", company=request.company_id, periods=request.interim_periods)

    provisions = []
    total_provision = 0.0
    cumulative_income = 0.0

    for i in range(request.interim_periods):
        period_income = request.period_income[i] if i < len(request.period_income) else 0
        cumulative_income += period_income
        annualized = period_income * (12 // (i + 1))
        provision = annualized * request.expected_annual_rate / request.interim_periods
        provisions.append(
            {
                "period": i + 1,
                "income": period_income,
                "annualized_income": annualized,
                "provision": round(provision, 2),
            }
        )
        total_provision += provision

    return TaxProvisionResponse(
        company_id=request.company_id,
        provision_periods=provisions,
        total_provision=round(total_provision, 2),
        deferred_tax_assets=round(sum(p.get("amount", 0) for p in request.temporary_differences) * 0.21, 2),
        deferred_tax_liabilities=round(sum(p.get("amount", 0) for p in request.permanent_differences) * 0.21, 2),
        effective_tax_rate=round(request.expected_annual_rate * 100, 2),
        reconciliation=[],
    )


@app.post("/transfer-pricing", response_model=TransferPricingResponse)
async def analyze_transfer_pricing(request: TransferPricingRequest):
    logger.info("Analyzing transfer pricing", company=request.company_id, party=request.related_party_id)

    comparable_avg = sum(c.get("price", request.transaction_amount) for c in request.comparable_data) / max(
        len(request.comparable_data), 1
    )
    adjustment = request.transaction_amount - comparable_avg

    return TransferPricingResponse(
        company_id=request.company_id,
        transaction_id=f"TP-{datetime.now().strftime('%Y%m%d')}",
        arm_length_amount=round(comparable_avg, 2),
        adjustment_required=round(adjustment, 2),
        method_used="Comparable Uncontrolled Price",
        range_low=round(comparable_avg * 0.9, 2),
        range_high=round(comparable_avg * 1.1, 2),
        compliance_status="compliant" if abs(adjustment) < comparable_avg * 0.05 else "review_required",
    )


@app.post("/compliance", response_model=TaxComplianceResponse)
async def file_tax_compliance(request: TaxComplianceRequest):
    logger.info("Filing tax compliance", company=request.company_id, jurisdiction=request.jurisdiction)

    total_amounts = sum(request.amounts.values())

    return TaxComplianceResponse(
        company_id=request.company_id,
        filing_id=f"FILING-{datetime.now().strftime('%Y%m%d%H%M')}",
        status="filed",
        filed_date=datetime.now(),
        amounts_confirmed=request.amounts,
        penalties_interest=0.0,
        next_filing_date=date(2024, 4, 15),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8346)
