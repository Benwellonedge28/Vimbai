"""
Biological Assets Service
Port: 8146
Tracks agricultural assets at fair value less costs to sell
"""
import httpx
import structlog
from datetime import datetime
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI, HTTPException

logger = structlog.get_logger()
app = FastAPI(title="Biological Assets Service", version="1.0.0")

# Pydantic Models
class BiologicalAsset(BaseModel):
    asset_id: str
    asset_name: str
    asset_category: str  # "bearer_biological", "consumable_biological"
    species: str
    quantity: float
    unit: str  # "head", "kg", "hectares"
    age_years: float
    maturity_status: str  # "immature", "mature"
    location: str
    acquisition_date: Optional[str] = None
    initial_cost: Optional[float] = None

class TransformationEntry(BaseModel):
    entry_date: str
    increase_type: str  # "growth", "birth", "purchase", "weight_gain"
    quantity_change: float
    fair_value_change: float
    costs_incurred: float
    description: str

class HarvestEntry(BaseModel):
    harvest_date: str
    quantity_harvested: float
    selling_price_per_unit: float
    costs_to_sell: float
    carrying_amount_at_harvest: float
    gain_loss_on_harvest: float

class BiologicalAssetRequest(BaseModel):
    company_id: str
    reporting_date: str
    assets: List[BiologicalAsset]
    transformations: List[TransformationEntry]
    harvests: List[HarvestEntry]
    include_disclosure: bool = True

class AssetFairValueMeasurement(BaseModel):
    asset_id: str
    asset_name: str
    category: str
    quantity: float
    unit: str
    age: float
    maturity: str
    market_price_per_unit: float
    estimated_costs_to_sell: float
    fair_value_per_unit: float
    total_fair_value: float

class GainLossBreakdown(BaseModel):
    asset_id: str
    physical_change: float
    price_change: float
    cost_change: float
    total_gain_loss: float
    recognized_in_PnL: float
    recognized_in_OCl: float

class BiologicalAssetsResponse(BaseModel):
    company_id: str
    reporting_date: str
    maturity_analysis: Dict[str, int]
    fair_value_measurements: List[AssetFairValueMeasurement]
    gain_loss_breakdown: List[GainLossBreakdown]
    total_biological_assets: float
    biological_asset_change: float
    harvested_amount: float
    agricultural_produce_amount: float
    costs_to_sell_total: float

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
    return {"status": "healthy", "service": "biological-assets", "version": "1.0.0"}

@app.post("/measure", response_model=BiologicalAssetsResponse)
async def measure_biological_assets(request: BiologicalAssetRequest):
    """Measure biological assets at fair value less costs to sell."""
    logger.info("Measuring biological assets", company=request.company_id, assets=len(request.assets))

    # Maturity analysis
    maturity = {"mature": 0, "immature": 0}

    # Fair value measurements
    fair_value_measurements = []
    total_fair_value = 0.0

    # Market prices by category and maturity (simulated)
    market_prices = {
        ("bearer_biological", "mature"): 5000.0,
        ("bearer_biological", "immature"): 2500.0,
        ("consumable_biological", "mature"): 800.0,
        ("consumable_biological", "immature"): 400.0
    }

    for asset in request.assets:
        maturity[asset.maturity_status] += 1

        price_key = (asset.asset_category, asset.maturity_status)
        market_price = market_prices.get(price_key, 1000.0)
        costs_to_sell_pct = 0.05  # 5% of market price
        fair_value_per_unit = market_price * (1 - costs_to_sell_pct)
        total_fv = fair_value_per_unit * asset.quantity

        total_fair_value += total_fv

        fair_value_measurements.append(AssetFairValueMeasurement(
            asset_id=asset.asset_id,
            asset_name=asset.asset_name,
            category=asset.asset_category,
            quantity=asset.quantity,
            unit=asset.unit,
            age=asset.age_years,
            maturity=asset.maturity_status,
            market_price_per_unit=market_price,
            estimated_costs_to_sell=market_price * costs_to_sold_pct * asset.quantity,
            fair_value_per_unit=fair_value_per_unit,
            total_fair_value=total_fv
        ))

    # Gain/loss breakdown
    gain_loss_breakdown = []
    total_gain = 0.0

    for trans in request.transformations:
        physical_change = trans.quantity_change * 500  # Simulated price
        price_change = trans.fair_value_change - physical_change
        total_gl = trans.fair_value_change

        gain_loss_breakdown.append(GainLossBreakdown(
            asset_id="TBD",
            physical_change=physical_change,
            price_change=price_change,
            cost_change=-trans.costs_incurred,
            total_gain_loss=total_gl,
            recognized_in_PnL=total_gl if trans.increase_type in ["harvest", "sale"] else 0,
            recognized_in_OCl=total_gl if trans.increase_type not in ["harvest", "sale"] else 0
        ))
        total_gain += total_gl

    # Harvest totals
    total_harvest_value = sum(h.quantity_harvested * h.selling_price_per_unit for h in request.harvests)
    total_costs_to_sell = sum(h.costs_to_sell for h in request.harvests)

    response = BiologicalAssetsResponse(
        company_id=request.company_id,
        reporting_date=request.reporting_date,
        maturity_analysis=maturity,
        fair_value_measurements=fair_value_measurements,
        gain_loss_breakdown=gain_loss_breakdown,
        total_biological_assets=total_fair_value,
        biological_asset_change=total_gain,
        harvested_amount=total_harvest_value,
        agricultural_produce_amount=total_harvest_value,
        costs_to_sell_total=total_costs_to_sell
    )

    logger.info("Biological assets measured", total_fair_value=total_fair_value)
    return response

@app.post("/fair-value-level1")
async def measure_level1_fair_value(
    asset_quantity: float,
    quoted_price_per_unit: float,
    costs_to_sell_percentage: float
):
    """Measure fair value at Level 1 (quoted price)."""
    costs_to_sell = asset_quantity * quoted_price_per_unit * costs_to_sell_percentage
    fair_value = (quoted_price_per_unit * asset_quantity) - costs_to_sell

    return {
        "level": 1,
        "measurement_technique": "Quoted price in active market",
        "asset_quantity": asset_quantity,
        "quoted_price_per_unit": quoted_price_per_unit,
        "costs_to_sell": costs_to_sell,
        "fair_value": fair_value,
        "fair_value_per_unit": quoted_price_per_unit * (1 - costs_to_sell_percentage)
    }

@app.post("/fair-value-level2")
async def measure_level2_fair_value(
    asset_quantity: float,
    market_price_similar: float,
    adjustment_factor: float,
    costs_to_sell_percentage: float
):
    """Measure fair value at Level 2 (market-comparable)."""
    adjusted_price = market_price_similar * adjustment_factor
    costs_to_sell = asset_quantity * adjusted_price * costs_to_sell_percentage
    fair_value = (adjusted_price * asset_quantity) - costs_to_sell

    return {
        "level": 2,
        "measurement_technique": "Market-comparable prices",
        "asset_quantity": asset_quantity,
        "market_price_similar": market_price_similar,
        "adjustment_factor": adjustment_factor,
        "adjusted_price": adjusted_price,
        "costs_to_sell": costs_to_sell,
        "fair_value": fair_value
    }

@app.post("/fair-value-level3")
async def measure_level3_fair_value(
    present_value_fcfe: float,
    terminal_value: float,
    costs_to_sell: float,
    discount_rate: float
):
    """Measure fair value at Level 3 (DCF model)."""
    total_value = present_value_fcfe + terminal_value
    fair_value = total_value - costs_to_sell

    return {
        "level": 3,
        "measurement_technique": "Discounted cash flows",
        "present_value_fcfe": present_value_fcfe,
        "terminal_value": terminal_value,
        "costs_to_sell": costs_to_sell,
        "fair_value": fair_value,
        "discount_rate": discount_rate,
        "key_unobservable_inputs": ["cash flow projections", "terminal growth rate", "discount rate"]
    }

@app.post("/consumable-bearer-distinction")
async def classify_asset(
    purpose: str,
    reproduction_purpose: str,
    harvesting_intent: str,
    historical_use: str
):
    """Classify biological asset as consumable or bearer."""
    if purpose == "sale" or harvesting_intent == "harvest" or historical_use == "harvest":
        classification = "consumable"
        description = "Biological asset to be harvested as agricultural produce or sold as biological asset"
    else:
        classification = "bearer"
        description = "Biological asset used to produce agricultural produce over multiple periods"

    return {
        "classification": classification,
        "description": description,
        "accounting_treatment": "Recognized at fair value less costs to sell" if classification == "consumable" else "Bearer biological assets also at fair value less costs to sell"
    }

@app.post("/disclosure-template")
async def prepare_disclosure(assets: List[BiologicalAsset], reporting_date: str):
    """Prepare biological assets disclosure template."""
    by_category = {}
    by_location = {}

    for asset in assets:
        by_category[asset.asset_category] = by_category.get(asset.asset_category, 0) + 1
        by_location[asset.location] = by_location.get(asset.location, 0) + 1

    return {
        "reporting_date": reporting_date,
        "disclosure_sections": [
            "Nature of activities",
            "Measurement basis (fair value less costs to sell)",
            "Financial risk management",
            "Quantitative disclosures by category",
            "Quantitative disclosures by location",
            "Gains/losses on fair value changes",
            "New assets acquired/developed",
            "Assets sold or harvested"
        ],
        "assets_by_category": by_category,
        "assets_by_location": by_location,
        "fair_value_hierarchy_required": True,
        "level_1_assets": 0,
        "level_2_assets": 0,
        "level_3_assets": len(assets)
    }

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8146)
