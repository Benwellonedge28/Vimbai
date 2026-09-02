"""
IFRS 15 Revenue Recognition Service
Port: 8148
Implements IFRS 15 five-step revenue model
"""

from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field

logger = structlog.get_logger()
app = FastAPI(title="IFRS 15 Revenue Recognition Service", version="1.0.0")


# Pydantic Models
class ContractDetails(BaseModel):
    contract_id: str
    customer_id: str
    contract_date: str
    performance_obligation_ids: List[str]
    transaction_price: float
    variable_consideration: float = 0.0
    significant_financing_component: bool = False
    non_cash_consideration: float = 0.0
    consideration_payable_customer: float = 0.0


class PerformanceObligation(BaseModel):
    obligation_id: str
    description: str
    obligation_type: str  # "point_in_time", "over_time"
    standalone_selling_price: float
    allocation_percentage: float
    satisfied: bool = False
    satisfaction_date: Optional[str] = None


class ContractAssetLiability(BaseModel):
    contract_id: str
    contract_asset: float
    contract_liability: float
    net_position: float


class RevenueRecognitionRequest(BaseModel):
    company_id: str
    reporting_date: str
    contracts: List[ContractDetails]
    performance_obligations: List[PerformanceObligation]
    actual_progress: Dict[str, float] = {}


class Step1Identification(BaseModel):
    contract_id: str
    contract_exists: bool
    probability_of_collection: float
    enforceability: bool
    has_commercial_substance: bool
    approved: bool
    reasons_for_rejection: List[str]


class Step2Obligations(BaseModel):
    contract_id: str
    distinct_goods_or_services: List[str]
    distinct_in_series: bool
    performance_obligations_identified: int
    bundled_obligations: List[str]


class Step3PriceAllocation(BaseModel):
    contract_id: str
    transaction_price: float
    allocations: Dict[str, float]
    variable_consideration_allocated: float
    remaining_obligations: List[str]


class Step4Satisfaction(BaseModel):
    obligation_id: str
    method: str  # "output", "input"
    progress_percentage: float
    satisfied_over_time: bool
    revenue_to_recognize: float


class IFRS15Response(BaseModel):
    company_id: str
    reporting_date: str
    step1_analysis: List[Step1Identification]
    step2_obligations: List[Step2Obligations]
    step3_allocations: List[Step3PriceAllocation]
    step4_satisfaction: List[Step4Satisfaction]
    contract_assets: List[ContractAssetLiability]
    total_revenue_recognized: float
    total_contract_liabilities: float
    remaining_performance_obligations: float


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
    return {"status": "healthy", "service": "ifrs-15-revenue-recognition", "version": "1.0.0"}


@app.post("/recognize", response_model=IFRS15Response)
async def recognize_revenue(request: RevenueRecognitionRequest):
    """Apply IFRS 15 five-step model to revenue recognition."""
    logger.info("Processing IFRS 15", company=request.company_id, contracts=len(request.contracts))

    step1_analysis = []
    step2_list = []
    step3_allocations = []
    step4_satisfaction = []
    contract_positions = []
    total_revenue = 0.0
    total_liabilities = 0.0
    remaining_rpo = 0.0

    for contract in request.contracts:
        # Step 1: Contract Identification
        collection_prob = 0.95  # Simulated
        step1_analysis.append(
            Step1Identification(
                contract_id=contract.contract_id,
                contract_exists=True,
                probability_of_collection=collection_prob,
                enforceability=True,
                has_commercial_substance=True,
                approved=collection_prob > 0.5,
                reasons_for_rejection=[],
            )
        )

        # Step 2: Identify Performance Obligations
        distinct_services = [
            po.description
            for po in request.performance_obligations
            if po.obligation_id in contract.performance_obligation_ids
        ]
        step2_list.append(
            Step2Obligations(
                contract_id=contract.contract_id,
                distinct_goods_or_services=distinct_services,
                distinct_in_series=False,
                performance_obligations_identified=len(contract.performance_obligation_ids),
                bundled_obligations=[],
            )
        )

        # Step 3: Determine Transaction Price
        total_ssp = sum(
            po.standalone_selling_price
            for po in request.performance_obligations
            if po.obligation_id in contract.performance_obligation_ids
        )
        allocations = {}
        for po in request.performance_obligations:
            if po.obligation_id in contract.performance_obligation_ids:
                alloc_pct = po.standalone_selling_price / total_ssp if total_ssp > 0 else 0
                allocations[po.obligation_id] = contract.transaction_price * alloc_pct

        step3_allocations.append(
            Step3PriceAllocation(
                contract_id=contract.contract_id,
                transaction_price=contract.transaction_price,
                allocations=allocations,
                variable_consideration_allocated=contract.variable_consideration,
                remaining_obligations=[
                    po_id for po_id in contract.performance_obligation_ids if po_id not in request.actual_progress
                ],
            )
        )

        # Step 4: Recognize Revenue
        contract_total_revenue = 0.0
        for po in request.performance_obligations:
            if po.obligation_id in contract.performance_obligation_ids:
                progress = request.actual_progress.get(po.obligation_id, 0.0)
                po_revenue = allocations.get(po.obligation_id, 0.0)

                if po.obligation_type == "over_time":
                    revenue_to_recognize = po_revenue * progress
                    remaining_rpo += po_revenue * (1 - progress)
                else:
                    revenue_to_recognize = po_revenue if progress == 1.0 else 0.0
                    remaining_rpo += po_revenue if progress < 1.0 else 0.0

                contract_total_revenue += revenue_to_recognize

                step4_satisfaction.append(
                    Step4Satisfaction(
                        obligation_id=po.obligation_id,
                        method="input" if po.obligation_type == "over_time" else "output",
                        progress_percentage=progress,
                        satisfied_over_time=po.obligation_type == "over_time" and progress == 1.0,
                        revenue_to_recognize=revenue_to_recognize,
                    )
                )

        total_revenue += contract_total_revenue
        total_liabilities += remaining_rpo

        # Contract asset/liability
        contract_positions.append(
            ContractAssetLiability(
                contract_id=contract.contract_id,
                contract_asset=max(0, contract_total_revenue - contract.transaction_price),
                contract_liability=max(0, contract.transaction_price - contract_total_revenue),
                net_position=contract_total_revenue - contract.transaction_price,
            )
        )

    response = IFRS15Response(
        company_id=request.company_id,
        reporting_date=request.reporting_date,
        step1_analysis=step1_analysis,
        step2_obligations=step2_list,
        step3_allocations=step3_allocations,
        step4_satisfaction=step4_satisfaction,
        contract_assets=contract_positions,
        total_revenue_recognized=total_revenue,
        total_contract_liabilities=total_liabilities,
        remaining_performance_obligations=remaining_rpo,
    )

    logger.info("IFRS 15 recognition complete", total_revenue=total_revenue)
    return response


@app.post("/variable-consideration")
async def estimate_variable_consideration(
    contract_id: str,
    variable_type: str,  # "sales_based", "milestone", "royalty"
    expected_value: float,
    most_likely_amount: float,
    constraint_applied: float,
):
    """Estimate and constrain variable consideration."""
    # IFRS 15 allows expected value or most likely amount methods
    best_estimate = most_likely_amount if variable_type == "sales_based" else expected_value

    # Apply constraint - only include if highly probable no significant reversal
    constrained_estimate = min(best_estimate, best_estimate * constraint_applied)

    return {
        "contract_id": contract_id,
        "variable_type": variable_type,
        "expected_value": expected_value,
        "most_likely_amount": most_likely_amount,
        "method_used": "most_likely amount" if variable_type == "sales_based" else "expected value",
        "pre_constraint": best_estimate,
        "constraint_applied": constraint_applied,
        "constrained_estimate": constrained_estimate,
        "reversal_risk": "low" if constrained_estimate == best_estimate else "constrained",
    }


@app.post("/standalone-selling-price")
async def calculate_ssp_allocation(
    obligation_id: str,
    observable_price: float,
    adjusted_market_assessment: float,
    expected_cost_plus_margin: float,
    allocation_percentage: float,
    transaction_price: float,
):
    """Calculate standalone selling price using different methods."""
    # All methods should give similar results if reliable
    ssp = adjusted_market_assessment  # Using adjusted market assessment as primary

    allocated_price = transaction_price * (allocation_percentage / 100)

    return {
        "obligation_id": obligation_id,
        "observable_price": observable_price,
        "adjusted_market_assessment": adjusted_market_assessment,
        "expected_cost_plus_margin": expected_cost_plus_margin,
        "recommended_ssp": ssp,
        "allocation_percentage": allocation_percentage,
        "allocated_transaction_price": allocated_price,
    }


@app.post("/progress-measurement")
async def measure_progress(
    obligation_id: str,
    method: str,  # "output_methods", "input_methods"
    units_delivered: int,
    total_units: int,
    costs_incurred: float,
    total_estimated_costs: float,
    resources_consumed: float,
    total_resources: float,
):
    """Measure progress towards satisfying performance obligation."""
    if method == "output_methods":
        if "units" in obligation_id:
            progress = units_delivered / total_units if total_units > 0 else 0
        else:
            progress = 0.5  # Default
    else:  # input methods
        progress = costs_incurred / total_estimated_costs if total_estimated_costs > 0 else 0
        # Adjust for non-productive time
        resources_progress = resources_consumed / total_resources if total_resources > 0 else 0
        progress = (progress + resources_progress) / 2

    progress_pct = min(1.0, max(0.0, progress))

    return {
        "obligation_id": obligation_id,
        "method_used": method,
        "progress_percentage": progress_pct * 100,
        "revenue_to_recognize": progress_pct,
        "costs_incurred": costs_incurred,
        "estimated_total_costs": total_estimated_costs,
        "expected_profit": (total_estimated_costs * progress_pct) - costs_incurred,
    }


@app.post("/customer-options")
async def assess_customer_options(
    option_type: str,  # "discount", "loyalty", "future_purchase"
    has_material_right: bool,
    standalone_selling_price: float,
    discount_rate: float,
    expected_exercise: float,
):
    """Assess whether customer options represent performance obligations."""
    if not has_material_right:
        return {
            "option_type": option_type,
            "is_performance_obligation": False,
            "treatment": "not a distinct good/service - no separate recognition",
        }

    # Material right = significant discount not available to all customers
    pv_ssp = standalone_selling_price * (1 - discount_rate) if discount_rate > 0 else standalone_selling_price
    exercise_probability = expected_exercise / 100

    allocated_consideration = pv_ssp * exercise_probability

    return {
        "option_type": option_type,
        "has_material_right": has_material_right,
        "is_performance_obligation": has_material_right,
        "standalone_selling_price": standalone_selling_price,
        "present_value": pv_ssp,
        "probability_adjusted_value": allocated_consideration,
        "treatment": "allocated to transaction price as material right",
    }


@app.post("/modification")
async def account_for_modification(
    original_contract_id: str,
    modification_type: str,  # "new_goods", "price_change", "scope_change"
    original_price: float,
    new_price: float,
    original_progress: float,
    modification_date: str,
):
    """Account for contract modifications under IFRS 15."""
    price_change = new_price - original_price
    scope_change = True if modification_type in ["new_goods", "scope_change"] else False

    if modification_type == "price_change":
        # Prospective treatment - adjust remaining revenue
        additional_revenue = price_change * (1 - original_progress)
        treatment = "prospective - cumulative catch-up adjustment"
    elif scope_change and price_change >= 0:
        # New contract or modification treated as separate
        additional_revenue = price_change
        treatment = "prospective - modification creates new contract"
    else:
        # Terminate and replace
        remaining_revenue = original_price * (1 - original_progress)
        additional_revenue = price_change
        treatment = "cumulative catch-up - modification does not create new contract"

    return {
        "original_contract_id": original_contract_id,
        "modification_type": modification_type,
        "original_price": original_price,
        "new_price": new_price,
        "price_change": price_change,
        "additional_revenue": additional_revenue,
        "treatment": treatment,
        "modification_date": modification_date,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8148)
