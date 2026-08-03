"""
Revaluation Model Service
Port: 8144
Implements revaluation model for property, plant and equipment under IFRS
"""
import httpx
import structlog
from datetime import datetime
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI, HTTPException

logger = structlog.get_logger()
app = FastAPI(title="Revaluation Model Service", version="1.0.0")

# Pydantic Models
class AssetDetails(BaseModel):
    asset_id: str
    asset_name: str
    asset_class: str  # "land", "building", "plant", "equipment", "vehicle"
    original_cost: float
    acquisition_date: str
    useful_life_years: int
    residual_value: float
    depreciation_method: str = "straight_line"
    accumulated_depreciation: float = 0.0

class RevaluationRequest(BaseModel):
    company_id: str
    revaluation_date: str
    assets: List[AssetDetails]
    fair_values: Dict[str, float]
    revaluation_model: str = "gross"  # "gross" or "net"
    include_frequency_check: bool = True
    regularity: str = "annual"  # "annual", "every_3_years", "every_5_years"

class RevaluationGain(BaseModel):
    asset_id: str
    asset_name: str
    previous_carrying_amount: float
    fair_value: float
    revaluation_gain: float
    treatment: str  # "to_revaluation_surplus", "to_profit_or_loss"
    cumulative_surplus: float

class RevaluationLoss(BaseModel):
    asset_id: str
    asset_name: str
    previous_carrying_amount: float
    fair_value: float
    revaluation_loss: float
    treatment: str  # "to_profit_or_loss", "against_previous_surplus", "mixed"
    absorbed_from_surplus: float
    charged_to_profit: float

class DepreciationAdjustment(BaseModel):
    asset_id: str
    new_useful_life: int
    remaining_useful_life: int
    current_depreciation_rate: float
    new_depreciation_rate: float
    new_annual_depreciation: float
    accumulated_depreciation_adjustment: float

class RevaluationResponse(BaseModel):
    company_id: str
    revaluation_date: str
    gains: List[RevaluationGain]
    losses: List[RevaluationLoss]
    depreciation_adjustments: List[DepreciationAdjustment]
    total_revaluation_gain: float
    total_revaluation_loss: float
    net_revaluation_effect: float
    revaluation_surplus_OCI: float
    profit_or_loss_impact: float
    new_carrying_amounts: Dict[str, float]

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
    return {"status": "healthy", "service": "revaluation-model", "version": "1.0.0"}

@app.post("/revalue", response_model=RevaluationResponse)
async def perform_revaluation(request: RevaluationRequest):
    """Perform revaluation of property, plant and equipment."""
    logger.info("Performing revaluation", company=request.company_id, date=request.revaluation_date)

    gains = []
    losses = []
    depreciation_adjustments = []
    total_gain = 0.0
    total_loss = 0.0
    cumulative_surplus = 0.0
    new_carrying_amounts = {}

    for asset in request.assets:
        fair_value = request.fair_values.get(asset.asset_id, asset.original_cost)

        # Calculate current carrying amount
        years_held = 2  # Simulated
        annual_depreciation = (asset.original_cost - asset.residual_value) / asset.useful_life_years
        current_carries = asset.original_cost - asset.accumulated_depreciation

        if fair_value > current_carries:
            # Revaluation gain
            gain = fair_value - current_carries
            total_gain += gain
            cumulative_surplus += gain

            gains.append(RevaluationGain(
                asset_id=asset.asset_id,
                asset_name=asset.asset_name,
                previous_carrying_amount=current_carries,
                fair_value=fair_value,
                revaluation_gain=gain,
                treatment="to_revaluation_surplus",
                cumulative_surplus=cumulative_surplus
            ))

            new_carrying_amounts[asset.asset_id] = fair_value

        elif fair_value < current_carries:
            # Revaluation loss
            loss = current_carries - fair_value

            # Check if there's previous surplus to absorb
            absorbed = min(loss, cumulative_surplus)
            charged = loss - absorbed
            cumulative_surplus = max(0, cumulative_surplus - absorbed)
            total_loss += loss

            losses.append(RevaluationLoss(
                asset_id=asset.asset_id,
                asset_name=asset.asset_name,
                previous_carrying_amount=current_carries,
                fair_value=fair_value,
                revaluation_loss=loss,
                treatment="mixed" if absorbed > 0 else "to_profit_or_loss",
                absorbed_from_surplus=absorbed,
                charged_to_profit=charged
            ))

            new_carrying_amounts[asset.asset_id] = fair_value

            # Depreciation adjustment
            remaining_life = max(1, asset.useful_life_years - years_held)
            new_annual_dep = (fair_value - asset.residual_value) / remaining_life
            old_annual_dep = annual_depreciation

            depreciation_adjustments.append(DepreciationAdjustment(
                asset_id=asset.asset_id,
                new_useful_life=asset.useful_life_years,
                remaining_useful_life=remaining_life,
                current_depreciation_rate=old_annual_dep / current_carries * 100,
                new_depreciation_rate=new_annual_dep / fair_value * 100,
                new_annual_depreciation=new_annual_dep,
                accumulated_depreciation_adjustment=0.0
            ))
        else:
            new_carrying_amounts[asset.asset_id] = current_carries

    response = RevaluationResponse(
        company_id=request.company_id,
        revaluation_date=request.revaluation_date,
        gains=gains,
        losses=losses,
        depreciation_adjustments=depreciation_adjustments,
        total_revaluation_gain=total_gain,
        total_revaluation_loss=total_loss,
        net_revaluation_effect=total_gain - total_loss,
        revaluation_surplus_OCI=total_gain - sum(l.absorbed_from_surplus for l in losses),
        profit_or_loss_impact=sum(l.charged_to_profit for l in losses),
        new_carrying_amounts=new_carrying_amounts
    )

    logger.info("Revaluation complete", gains=len(gains), losses=len(losses), net_effect=response.net_revaluation_effect)
    return response

@app.post("/revaluation-surplus")
async def calculate_revaluation_surplus(
    revaluation_gain: float,
    deferred_tax_rate: float,
    previous_surplus: float = 0.0,
    losses_absorbed: float = 0.0
):
    """Calculate revaluation surplus and its tax effects."""
    net_gain_after_losses = revaluation_gain - losses_absorbed
    deferred_tax = net_gain_after_losses * deferred_tax_rate
    revaluation_surplus = net_gain_after_losses - deferred_tax

    return {
        "gross_revaluation_gain": revaluation_gain,
        "losses_absorbed_from_surplus": losses_absorbed,
        "net_gain": net_gain_after_losses,
        "deferred_tax_liability": deferred_tax,
        "revaluation_surplus_OCI": revaluation_surplus,
        "previous_cumulative_surplus": previous_surplus,
        "total_revaluation_surplus": previous_surplus + revaluation_surplus
    }

@app.post("/depreciation-after-revaluation")
async def calculate_depreciation_after_revaluation(
    revalued_amount: float,
    residual_value: float,
    remaining_useful_life: int,
    total_useful_life: int
):
    """Calculate depreciation charge after revaluation."""
    depreciable_amount = revalued_amount - residual_value
    annual_depreciation = depreciable_amount / remaining_useful_life if remaining_useful_life > 0 else 0
    depreciation_rate = annual_depreciation / depreciable_amount * 100 if depreciable_amount > 0 else 0

    return {
        "revalued_amount": revalued_amount,
        "residual_value": residual_value,
        "depreciable_amount": depreciable_amount,
        "remaining_useful_life": remaining_useful_life,
        "annual_depreciation": annual_depreciation,
        "depreciation_rate": depreciation_rate,
        "monthly_depreciation": annual_depreciation / 12,
        "total_life_expired": ((total_useful_life - remaining_useful_life) / total_useful_life * 100) if total_useful_life > 0 else 0
    }

@app.post("/impairment-after-revaluation")
async def calculate_impairment_after_revaluation(
    carrying_amount: float,
    recoverable_amount: float,
    revaluation_surplus_available: float
):
    """Calculate impairment loss after revaluation."""
    impairment_loss = carrying_amount - recoverable_amount
    if impairment_loss <= 0:
        return {
            "carrying_amount": carrying_amount,
            "recoverable_amount": recoverable_amount,
            "impairment_loss": 0,
            "recoverable_amount_is_higher": True,
            "no_impairment_required": True
        }

    # First charge against revaluation surplus
    absorbed_from_surplus = min(impairment_loss, revaluation_surplus_available)
    charged_to_profit = impairment_loss - absorbed_from_surplus

    return {
        "carrying_amount": carrying_amount,
        "recoverable_amount": recoverable_amount,
        "impairment_loss": impairment_loss,
        "absorbed_from_revaluation_surplus": absorbed_from_surplus,
        "charged_to_profit_or_loss": charged_to_profit,
        "remaining_surplus": revaluation_surplus_available - absorbed_from_surplus
    }

@app.post("/indexation-allowance")
async def calculate_indexation_allowance(
    original_cost: float,
    original_date: str,
    disposal_date: str,
    cpi_at_acquisition: float,
    cpi_at_disposal: float
):
    """Calculate indexation allowance for capital gains."""
    indexation_factor = cpi_at_disposal / cpi_at_acquisition if cpi_at_acquisition > 0 else 1.0
    indexed_cost = original_cost * indexation_factor
    indexation_allowance = indexed_cost - original_cost

    return {
        "original_cost": original_cost,
        "original_date": original_date,
        "disposal_date": disposal_date,
        "cpi_at_acquisition": cpi_at_acquisition,
        "cpi_at_disposal": cpi_at_disposal,
        "indexation_factor": indexation_factor,
        "indexed_cost": indexed_cost,
        "indexation_allowance": indexation_allowance
    }

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8144)
