"""
IFRS 16 Lease Accounting Service
Port: 8147
Implements IFRS 16 lease accounting, calculates ROU assets and lease liabilities
"""

from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field

logger = structlog.get_logger()
app = FastAPI(title="IFRS 16 Lease Accounting Service", version="1.0.0")


# Pydantic Models
class LeaseDetails(BaseModel):
    lease_id: str
    lessee_name: str
    lease_type: str  # "operating", "finance", "short_term", "low_value"
    asset_class: str  # "property", "vehicle", "equipment", "IT"
    lease_commencement_date: str
    lease_term_years: int
    payment_frequency: str = "monthly"
    payment_amount: float
    number_of_payments: int
    discount_rate: float = Field(ge=0, le=1)
    initial_direct_costs: float = 0.0
    lease_incentive_received: float = 0.0
    variable_lease_payments: float = 0.0
    renewal_options_probable: bool = False
    guaranteed_residual_value: float = 0.0


class IFRS16CalculationRequest(BaseModel):
    company_id: str
    reporting_date: str
    leases: List[LeaseDetails]
    incremental_borrowing_rate: float
    include_short_term_exemption: bool = True
    include_low_value_exemption: bool = True


class LeaseLiabilityMeasurement(BaseModel):
    lease_id: str
    lease_commencement_date: str
    lease_term_years: int
    payment_amount: float
    number_of_payments: int
    discount_rate: float
    present_value_lease_payments: float
    guaranteed_residual_value: float
    initial_measurement: float


class ROUAssetMeasurement(BaseModel):
    lease_id: str
    initial_lease_liability: float
    initial_direct_costs: float
    lease_incentive_received: float
    prepaid_lease_payments: float
    initial_recognition: float
    depreciation_period: int
    annual_depreciation: float


class LeaseExpenseBreakdown(BaseModel):
    lease_id: str
    interest_expense: float
    depreciation_charge: float
    variable_lease_payment: float
    short_term_lease_expense: float
    low_value_lease_expense: float
    total_lease_expense: float


class IFRS16Response(BaseModel):
    company_id: str
    reporting_date: str
    total_lease_liability: float
    total_rou_assets: float
    lease_liabilities: List[LeaseLiabilityMeasurement]
    rou_assets: List[ROUAssetMeasurement]
    lease_expenses: List[LeaseExpenseBreakdown]
    interest_payable_current: float
    interest_payable_non_current: float
    total_interest_expense: float


async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    """Call another internal Vimbai service."""
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            if data:
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "ifrs-16-lease-accounting", "version": "1.0.0"}


@app.post("/calculate", response_model=IFRS16Response)
async def calculate_ifrs16(request: IFRS16CalculationRequest):
    """Calculate IFRS 16 lease accounting entries."""
    logger.info("Calculating IFRS 16", company=request.company_id, leases=len(request.leases))

    lease_liabilities = []
    rou_assets = []
    lease_expenses = []
    total_liability = 0.0
    total_rou = 0.0
    total_interest = 0.0

    for lease in request.leases:
        # Skip short-term and low-value leases if exemptions apply
        if request.include_short_term_exemption and lease.lease_type == "short_term":
            # Expense only treatment
            annual_expense = lease.payment_amount * 12
            lease_expenses.append(
                LeaseExpenseBreakdown(
                    lease_id=lease.lease_id,
                    interest_expense=0.0,
                    depreciation_charge=0.0,
                    variable_lease_payment=lease.variable_lease_payments,
                    short_term_lease_expense=annual_expense,
                    low_value_lease_expense=0.0,
                    total_lease_expense=annual_expense,
                )
            )
            continue

        if request.include_low_value_exemption and lease.lease_type == "low_value":
            annual_expense = lease.payment_amount * 12
            lease_expenses.append(
                LeaseExpenseBreakdown(
                    lease_id=lease.lease_id,
                    interest_expense=0.0,
                    depreciation_charge=0.0,
                    variable_lease_payment=lease.variable_lease_payments,
                    short_term_lease_expense=0.0,
                    low_value_lease_expense=annual_expense,
                    total_lease_expense=annual_expense,
                )
            )
            continue

        # Calculate present value of lease payments
        n = lease.number_of_payments
        r = lease.discount_rate
        pv_payments = lease.payment_amount * ((1 - (1 + r) ** (-n)) / r) if r > 0 else lease.payment_amount * n
        pv_residual = lease.guaranteed_residual_value / ((1 + r) ** n) if r > 0 else lease.guaranteed_residual_value
        pv_total = pv_payments + pv_residual

        # Lease liability
        lease_liabilities.append(
            LeaseLiabilityMeasurement(
                lease_id=lease.lease_id,
                lease_commencement_date=lease.lease_commencement_date,
                lease_term_years=lease.lease_term_years,
                payment_amount=lease.payment_amount,
                number_of_payments=lease.number_of_payments,
                discount_rate=lease.discount_rate,
                present_value_lease_payments=pv_payments,
                guaranteed_residual_value=lease.guaranteed_residual_value,
                initial_measurement=pv_total,
            )
        )
        total_liability += pv_total

        # ROU Asset
        initial_rou = pv_total + lease.initial_direct_costs - lease.lease_incentive_received
        depreciation_period = (
            lease.lease_term_years if not lease.renewal_options_probable else lease.lease_term_years + 2
        )
        annual_dep = initial_rou / depreciation_period

        rou_assets.append(
            ROUAssetMeasurement(
                lease_id=lease.lease_id,
                initial_lease_liability=pv_total,
                initial_direct_costs=lease.initial_direct_costs,
                lease_incentive_received=lease.lease_incentive_received,
                prepaid_lease_payments=0.0,
                initial_recognition=initial_rou,
                depreciation_period=depreciation_period,
                annual_depreciation=annual_dep,
            )
        )
        total_rou += initial_rou

        # Interest expense (first year)
        year1_interest = pv_total * r
        total_interest += year1_interest

        lease_expenses.append(
            LeaseExpenseBreakdown(
                lease_id=lease.lease_id,
                interest_expense=year1_interest,
                depreciation_charge=annual_dep,
                variable_lease_payment=lease.variable_lease_payments,
                short_term_lease_expense=0.0,
                low_value_lease_expense=0.0,
                total_lease_expense=year1_interest + annual_dep + lease.variable_lease_payments,
            )
        )

    response = IFRS16Response(
        company_id=request.company_id,
        reporting_date=request.reporting_date,
        total_lease_liability=total_liability,
        total_rou_assets=total_rou,
        lease_liabilities=lease_liabilities,
        rou_assets=rou_assets,
        lease_expenses=lease_expenses,
        interest_payable_current=total_interest * 0.3,
        interest_payable_non_current=total_interest * 0.7,
        total_interest_expense=total_interest,
    )

    logger.info("IFRS 16 calculation complete", total_liability=total_liability, total_rou=total_rou)
    return response


@app.post("/lease-liability")
async def measure_lease_liability(
    payment_amount: float, number_of_payments: int, discount_rate: float, guaranteed_residual: float = 0.0
):
    """Measure lease liability using present value."""
    r = discount_rate
    n = number_of_payments

    # PV of payments
    if r > 0:
        pv_payments = payment_amount * ((1 - (1 + r) ** (-n)) / r)
        pv_residual = guaranteed_residual / ((1 + r) ** n)
    else:
        pv_payments = payment_amount * n
        pv_residual = guaranteed_residual

    pv_total = pv_payments + pv_residual

    # Amortization schedule
    schedule = []
    remaining = pv_total
    for period in range(1, n + 1):
        interest = remaining * r
        principal = payment_amount - interest
        remaining = max(0, remaining - principal)
        schedule.append(
            {
                "period": period,
                "payment": payment_amount,
                "interest": interest,
                "principal": principal,
                "remaining_balance": remaining,
            }
        )

    return {
        "present_value_lease_payments": pv_payments,
        "present_value_residual": pv_residual,
        "total_lease_liability": pv_total,
        "amortization_schedule": schedule,
        "total_interest": sum(s["interest"] for s in schedule),
    }


@app.post("/renewal-options")
async def assess_renewal_options(
    renewal_option_years: int,
    market_rent_annual: float,
    current_rent_annual: float,
    economic_incentives: str,
    business_use_intent: str,
):
    """Assess whether renewal options should be included in lease term."""
    # Criteria: economic penalties, business use, renewal intent
    market_advantage = (market_rent_annual - current_rent_annual) / market_rent_annual if market_rent_annual > 0 else 0

    probable_criteria_met = False
    if economic_incentives == "significant" and business_use_intent == "continue":
        probable_criteria_met = True
    elif economic_incentives == "moderate" and market_advantage > 0.1:
        probable_criteria_met = True

    return {
        "renewal_option_years": renewal_option_years,
        "market_rent_annual": market_rent_annual,
        "current_rent_annual": current_rent_annual,
        "economic_incentives": economic_incentives,
        "business_use_intent": business_use_intent,
        "renewal_probable": probable_criteria_met,
        "recommended_lease_term_extension": renewal_option_years if probable_criteria_met else 0,
        "reasoning": (
            "Renewal options included" if probable_criteria_met else "Renewal options excluded from lease term"
        ),
    }


@app.post("/variable-lease-payments")
async def calculate_variable_payments(
    lease_id: str, variable_payment_type: str, base_amount: float, variable_rate: float, usage_measure: float
):
    """Calculate variable lease payments."""
    if variable_payment_type == "percentage_of_sales":
        variable_payment = base_amount * (variable_rate / 100)
    elif variable_payment_type == "usage_based":
        variable_payment = base_amount * usage_measure
    elif variable_payment_type == "index_rate":
        variable_payment = base_amount * (1 + variable_rate)
    else:
        variable_payment = 0.0

    return {
        "lease_id": lease_id,
        "variable_payment_type": variable_payment_type,
        "base_amount": base_amount,
        "variable_rate": variable_rate,
        "usage_measure": usage_measure,
        "variable_lease_payment": variable_payment,
        "recognition": (
            "expensed as incurred" if variable_payment_type != "percentage_of_sales" else "included in lease liability"
        ),
    }


@app.post("/sale-leaseback")
async def calculate_sale_leaseback(
    sale_price: float,
    carrying_amount: float,
    fair_value: float,
    lease_payment: float,
    lease_term: int,
    discount_rate: float,
):
    """Calculate sale and leaseback transaction."""
    profit_on_sale = sale_price - carrying_amount
    sale_at_fair_value = profit_on_sale if sale_price == fair_value else 0
    sale_below_fair_value = (fair_value - sale_price) if sale_price < fair_value else 0
    sale_above_fair_value = (sale_price - fair_value) if sale_price > fair_value else 0

    # PV of lease payments
    n = lease_term * 12  # Monthly payments
    r = discount_rate / 12
    pv_lease = lease_payment * ((1 - (1 + r) ** (-n)) / r)

    transfer_qualified = pv_lease >= fair_value * 0.9  # 90% threshold

    return {
        "sale_price": sale_price,
        "carrying_amount": carrying_amount,
        "fair_value": fair_value,
        "profit_on_sale": profit_on_sale,
        "sale_at_fair_value": sale_at_fair_value,
        "sale_below_fair_value": sale_below_fair_value,
        "sale_above_fair_value": sale_above_fair_value,
        "right_of_use_asset_recognized": pv_lease if transfer_qualified else fair_value,
        "gain_deferred": sale_below_fair_value,
        "gain_recognized_immediately": sale_at_fair_value,
        "transfer_qualifies_as_sale": transfer_qualified,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8147)
