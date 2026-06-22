"""
Investment Property Service
Port: 8145
Accounts for investment property using fair value model or cost model
"""
import httpx
import structlog
from datetime import datetime
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI, HTTPException

logger = structlog.get_logger()
app = FastAPI(title="Investment Property Service", version="1.0.0")

# Pydantic Models
class InvestmentProperty(BaseModel):
    property_id: str
    property_name: str
    property_type: str  # "land", "building", "mixed"
    acquisition_date: str
    acquisition_cost: float
    fair_value_last_reporting: float
    useful_life_years: int = 50
    residual_value: float = 0.0
    depreciation_method: str = "straight_line"

class FairValueModelRequest(BaseModel):
    company_id: str
    reporting_date: str
    properties: List[InvestmentProperty]
    current_fair_values: Dict[str, float]
    rental_income: float
    direct_operating_expenses: float

class CostModelRequest(BaseModel):
    company_id: str
    reporting_date: str
    properties: List[InvestmentProperty]
    depreciation_rate: float

class FairValueMeasurement(BaseModel):
    property_id: str
    property_name: str
    opening_fair_value: float
    additions: float
    disposals: float
    fair_value_gain_loss: float
    closing_fair_value: float
    rental_income: float
    direct_expenses: float
    net_income: float

class CostModelDepreciation(BaseModel):
    property_id: str
    property_name: str
    cost: float
    accumulated_depreciation_opening: float
    depreciation_for_period: float
    accumulated_depreciation_closing: float
    disposal_proceeds: float
    gain_loss_on_disposal: float
    carrying_amount: float

class InvestmentPropertyResponse(BaseModel):
    company_id: str
    reporting_date: str
    accounting_model: str
    fair_value_measurements: Optional[List[FairValueMeasurement]] = None
    cost_model_depreciations: Optional[List[CostModelDepreciation]] = None
    total_fair_value_or_carrying_amount: float
    fair_value_gain_loss_in_statement_of_profit_or_loss: float
    fair_value_gain_loss_in_OCI: float
    direct_expenses_including_depreciation: float

async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    """Call another internal FinAcc service."""
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
    return {"status": "healthy", "service": "investment-property", "version": "1.0.0"}

@app.post("/fair-value-model", response_model=InvestmentPropertyResponse)
async def apply_fair_value_model(request: FairValueModelRequest):
    """Apply fair value model to investment property."""
    logger.info("Applying fair value model", company=request.company_id, properties=len(request.properties))

    fair_value_measurements = []
    total_fair_value = 0.0
    total_fair_value_gain = 0.0

    for prop in request.properties:
        current_fair_value = request.current_fair_values.get(prop.property_id, prop.fair_value_last_reporting)

        opening = prop.fair_value_last_reporting
        additions = 0.0  # No additions in this period
        disposals = 0.0
        fv_gain_loss = current_fair_value - opening

        total_fair_value += current_fair_value
        total_fair_value_gain += fv_gain_loss

        # Allocate rental income and expenses proportionally
        prop_rental = request.rental_income * (opening / sum(p.fair_value_last_reporting for p in request.properties)) if request.properties else 0
        prop_expenses = request.direct_operating_expenses * (opening / sum(p.fair_value_last_reporting for p in request.properties)) if request.properties else 0

        fair_value_measurements.append(FairValueMeasurement(
            property_id=prop.property_id,
            property_name=prop.property_name,
            opening_fair_value=opening,
            additions=additions,
            disposals=disposals,
            fair_value_gain_loss=fv_gain_loss,
            closing_fair_value=current_fair_value,
            rental_income=prop_rental,
            direct_expenses=prop_expenses,
            net_income=prop_rental - prop_expenses
        ))

    response = InvestmentPropertyResponse(
        company_id=request.company_id,
        reporting_date=request.reporting_date,
        accounting_model="fair_value",
        fair_value_measurements=fair_value_measurements,
        cost_model_depreciations=None,
        total_fair_value_or_carrying_amount=total_fair_value,
        fair_value_gain_loss_in_statement_of_profit_or_loss=0.0,  # Recognized in OCI/revaluation surplus
        fair_value_gain_loss_in_OCI=total_fair_value_gain,
        direct_expenses_including_depreciation=request.direct_operating_expenses
    )

    logger.info("Fair value model applied", total_fair_value=total_fair_value, fv_gain=total_fair_value_gain)
    return response

@app.post("/cost-model", response_model=InvestmentPropertyResponse)
async def apply_cost_model(request: CostModelRequest):
    """Apply cost model to investment property."""
    logger.info("Applying cost model", company=request.company_id, properties=len(request.properties))

    cost_model_depreciations = []
    total_carrying_amount = 0.0

    for prop in request.properties:
        annual_depreciation = (prop.acquisition_cost - prop.residual_value) / prop.useful_life_years
        accumulated_opening = annual_depreciation * 2  # Assume 2 years old
        depreciation_period = annual_depreciation
        accumulated_closing = accumulated_opening + depreciation_period

        # Simulated disposal
        disposal_proceeds = prop.acquisition_cost * 1.1
        carrying_amount_disposal = prop.acquisition_cost - accumulated_closing
        gain_loss = disposal_proceeds - carrying_amount_disposal

        carrying_amount = prop.acquisition_cost - accumulated_closing
        total_carrying_amount += carrying_amount

        cost_model_depreciations.append(CostModelDepreciation(
            property_id=prop.property_id,
            property_name=prop.property_name,
            cost=prop.acquisition_cost,
            accumulated_depreciation_opening=accumulated_opening,
            depreciation_for_period=depreciation_period,
            accumulated_depreciation_closing=accumulated_closing,
            disposal_proceeds=disposal_proceeds,
            gain_loss_on_disposal=gain_loss,
            carrying_amount=carrying_amount
        ))

    response = InvestmentPropertyResponse(
        company_id=request.company_id,
        reporting_date=request.reporting_date,
        accounting_model="cost",
        fair_value_measurements=None,
        cost_model_depreciations=cost_model_depreciations,
        total_fair_value_or_carrying_amount=total_carrying_amount,
        fair_value_gain_loss_in_statement_of_profit_or_loss=0.0,
        fair_value_gain_loss_in_OCI=0.0,
        direct_expenses_including_depreciation=sum(d.depreciation_for_period for d in cost_model_depreciations)
    )

    logger.info("Cost model applied", total_carrying=total_carrying_amount)
    return response

@app.post("/transfer")
async def process_transfer(
    property_id: str,
    reason_for_transfer: str,
    carrying_amount: float,
    fair_value: float,
    model_from: str,
    model_to: str
):
    """Process transfer between owner-occupied property and investment property."""
    # IFRS requires consistent treatment

    if model_from == "owner_occupied" and model_to == "investment":
        # Transfer at fair value
        adjustment = fair_value - carrying_amount
        return {
            "property_id": property_id,
            "transfer_date": datetime.now().date().isoformat(),
            "reason": reason_for_transfer,
            "carrying_amount_before_transfer": carrying_amount,
            "fair_value_at_transfer": fair_value,
            "adjustment_to_property": adjustment,
            "treatment": "recognized in OCI" if adjustment > 0 else "recognized in P&L"
        }
    else:
        # Transfer from investment property
        return {
            "property_id": property_id,
            "transfer_date": datetime.now().date().isoformat(),
            "reason": reason_for_transfer,
            "carrying_amount": carrying_amount,
            "treatment": "cost becomes deemed cost for subsequent measurement"
        }

@app.post("/fair-value-disclosure")
async def prepare_fair_value_disclosure(properties: List[InvestmentProperty], current_fair_values: Dict[str, float]):
    """Prepare fair value disclosure for investment property."""
    level_1 = []  # Quoted prices
    level_2 = []  # Observable inputs
    level_3 = []  # Unobservable inputs

    for prop in properties:
        fair_value = current_fair_values.get(prop.property_id, prop.fair_value_last_reporting)
        item = {"property_id": prop.property_id, "fair_value": fair_value}

        # Classify by hierarchy (simulated)
        if fair_value > 10000000:
            level_3.append(item)
        elif fair_value > 1000000:
            level_2.append(item)
        else:
            level_1.append(item)

    return {
        "total_properties": len(properties),
        "level_1_fair_value": sum(l["fair_value"] for l in level_1),
        "level_2_fair_value": sum(l["fair_value"] for l in level_2),
        "level_3_fair_value": sum(l["fair_value"] for l in level_3),
        "level_1_count": len(level_1),
        "level_2_count": len(level_2),
        "level_3_count": len(level_3),
        "valuation_technique": "Discounted cash flows" if level_3 else "Market comparison",
        "significant_unobservable_inputs": ["DCF rate", "Occupancy rate", "Growth rate"] if level_3 else []
    }

@app.post("/yields")
async def calculate_property_yields(
    rental_income_annual: float,
    current_fair_value: float,
    capitalization_rate: float
):
    """Calculate investment property yields."""
    gross_yield = rental_income_annual / current_fair_value * 100
    net_yield = (rental_income_annual - (rental_income_annual * 0.3)) / current_fair_value * 100  # 30% operating expenses
    market_value_implied = rental_income_annual / (capitalization_rate / 100) if capitalization_rate > 0 else 0

    return {
        "rental_income_annual": rental_income_annual,
        "current_fair_value": current_fair_value,
        "gross_yield": gross_yield,
        "net_yield": net_yield,
        "capitalization_rate": capitalization_rate,
        "market_value_implied": market_value_implied,
        "capital_growth": (current_fair_value - market_value_implied) / market_value_implied * 100 if market_value_implied > 0 else 0
    }

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8145)
