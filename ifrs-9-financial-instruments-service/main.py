"""
IFRS 9 Financial Instruments Service
Port: 8149
Handles IFRS 9 classification, measurement, and impairment (ECL model)
"""

from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field

logger = structlog.get_logger()
app = FastAPI(title="IFRS 9 Financial Instruments Service", version="1.0.0")


# Pydantic Models
class FinancialAsset(BaseModel):
    asset_id: str
    asset_name: str
    asset_type: str  # "debt", "equity", "derivative"
    category: str  # "amortized_cost", "FVOCI", "FVTPL"
    carrying_amount: float
    face_value: float
    effective_interest_rate: float
    maturity_date: str
    credit_rating: str
    expected_credit_loss_stage: int = Field(ge=1, le=3)  # 1=Stage 1, 2=Stage 2, 3=Stage 3


class ECLCalculation(BaseModel):
    asset_id: str
    probability_of_default: float
    loss_given_default: float
    exposure_at_default: float
    lifetime_pd: float
    stage_1_ecl: float
    stage_2_ecl: float
    stage_3_ecl: float
    impairment: float


class IFRS9Request(BaseModel):
    company_id: str
    reporting_date: str
    financial_assets: List[FinancialAsset]
    forward_looking_factors: Dict[str, float] = {}
    base_pdg: float = 0.01
    base_lgd: float = 0.45
    include_macroeconomic: bool = True


class ClassificationResult(BaseModel):
    asset_id: str
    business_model: str  # "hold_to_collect", "hold_to_collect_sell", "trading"
    sppi_test: str  # "passed", "failed"
    classification: str
    measurement_basis: str
    reclassification_required: bool


class MeasurementResult(BaseModel):
    asset_id: str
    initial_measurement: float
    effective_interest_rate: float
    interest_revenue: float
    impairment_charge: float
    carrying_amount_end: float
    fair_value_change: float


class IFRS9Response(BaseModel):
    company_id: str
    reporting_date: str
    classifications: List[ClassificationResult]
    ecl_calculations: List[ECLCalculation]
    measurements: List[MeasurementResult]
    total_amortized_cost: float
    total_FVOCI: float
    total_FVTPL: float
    total_ECL: float
    stage_1_ECL: float
    stage_2_ECL: float
    stage_3_ECL: float


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
    return {"status": "healthy", "service": "ifrs-9-financial-instruments", "version": "1.0.0"}


@app.post("/classify", response_model=IFRS9Response)
async def classify_and_measure(request: IFRS9Request):
    """Classify and measure financial assets under IFRS 9."""
    logger.info("Processing IFRS 9", company=request.company_id, assets=len(request.financial_assets))

    classifications = []
    ecl_calculations = []
    measurements = []
    total_amortized = 0.0
    total_fvoci = 0.0
    total_fvtpl = 0.0
    stage_1_ecl = 0.0
    stage_2_ecl = 0.0
    stage_3_ecl = 0.0

    for asset in request.financial_assets:
        # Classification
        sppi_result = "passed" if asset.asset_type != "derivative" else "failed"
        business_model = "hold_to_collect" if asset.category == "amortized_cost" else "hold_to_collect_sell"

        classifications.append(
            ClassificationResult(
                asset_id=asset.asset_id,
                business_model=business_model,
                sppi_test=sppi_result,
                classification=asset.category,
                measurement_basis=asset.category,
                reclassification_required=False,
            )
        )

        # ECL Calculation
        base_pd = request.base_pdg
        base_lgd = request.base_lgd

        # Stage-based PD adjustment
        if asset.expected_credit_loss_stage == 1:
            pd = base_pd
            lifetime_pd = base_pd * 3
            e1 = asset.carrying_amount * pd * base_lgd
            e2 = asset.carrying_amount * lifetime_pd * base_lgd * 0.1
            e3 = 0.0
            stage_1_ecl += e1
        elif asset.expected_credit_loss_stage == 2:
            pd = base_pd * 2.5
            lifetime_pd = base_pd * 10
            e1 = 0.0
            e2 = asset.carrying_amount * lifetime_pd * base_lgd
            e3 = 0.0
            stage_2_ecl += e2
        else:  # Stage 3
            pd = 1.0
            lifetime_pd = 1.0
            e1 = 0.0
            e2 = 0.0
            e3 = asset.carrying_amount * base_lgd
            stage_3_ecl += e3

        ead = asset.carrying_amount

        ecl_calculations.append(
            ECLCalculation(
                asset_id=asset.asset_id,
                probability_of_default=pd,
                loss_given_default=base_lgd,
                exposure_at_default=ead,
                lifetime_pd=lifetime_pd,
                stage_1_ecl=e1,
                stage_2_ecl=e2,
                stage_3_ecl=e3,
                impairment=max(e1, e2, e3),
            )
        )

        # Measurement
        interest_revenue = asset.carrying_amount * asset.effective_interest_rate
        carrying_after_impairment = asset.carrying_amount - max(e1, e2, e3)
        fair_value_change = asset.carrying_amount * 0.02  # Simulated

        measurements.append(
            MeasurementResult(
                asset_id=asset.asset_id,
                initial_measurement=asset.carrying_amount,
                effective_interest_rate=asset.effective_interest_rate,
                interest_revenue=interest_revenue,
                impairment_charge=max(e1, e2, e3),
                carrying_amount_end=carrying_after_impairment,
                fair_value_change=fair_value_change if asset.category != "amortized_cost" else 0.0,
            )
        )

        if asset.category == "amortized_cost":
            total_amortized += carrying_after_impairment
        elif asset.category == "FVOCI":
            total_fvoci += carrying_after_impairment
        else:
            total_fvtpl += carrying_after_impairment + fair_value_change

    total_ecl = stage_1_ecl + stage_2_ecl + stage_3_ecl

    response = IFRS9Response(
        company_id=request.company_id,
        reporting_date=request.reporting_date,
        classifications=classifications,
        ecl_calculations=ecl_calculations,
        measurements=measurements,
        total_amortized_cost=total_amortized,
        total_FVOCI=total_fvoci,
        total_FVTPL=total_fvtpl,
        total_ECL=total_ecl,
        stage_1_ECL=stage_1_ecl,
        stage_2_ECL=stage_2_ecl,
        stage_3_ECL=stage_3_ecl,
    )

    logger.info("IFRS 9 processing complete", total_ecl=total_ecl)
    return response


@app.post("/sppi-test")
async def perform_sppi_test(
    instrument_type: str,
    principal_amount: float,
    interest_type: str,  # "fixed", "variable", "contingent"
    contingency_feature: str,
    not_ional_linked: bool,
):
    """Perform Solely Payments of Principal and Interest test."""
    test_result = True
    failed_criteria = []

    if interest_type == "contingent":
        test_result = False
        failed_criteria.append("Contingent interest payments")

    if contingency_feature == "credit_sensitive":
        test_result = False
        failed_criteria.append("Credit-sensitive payments")

    if not_ional_linked:
        test_result = False
        failed_criteria.append("Returns linked to notional")

    return {
        "instrument_type": instrument_type,
        "principal_amount": principal_amount,
        "sppi_test_passed": test_result,
        "failed_criteria": failed_criteria,
        "classification_if_passed": "amortized_cost or FVOCI",
        "classification_if_failed": "FVTPL",
    }


@app.post("/business-model")
async def assess_business_model(
    holding_purpose: str,  # "collect_principal", "collect_and_sell", "trading"
    sales_frequency: str,  # "rare", "occasional", "frequent"
    sales_proceeds: float,
    total_income: float,
):
    """Assess business model for financial assets."""
    if holding_purpose == "collect_principal":
        model = "hold to collect"
        measurement = "amortized_cost"
        reclassification = False
    elif holding_purpose == "collect_and_sell" or sales_frequency == "frequent":
        model = "hold to collect and sell"
        measurement = "FVOCI"
        reclassification = False
    else:
        model = "trading or other"
        measurement = "FVTPL"
        reclassification = False

    return {
        "holding_purpose": holding_purpose,
        "sales_frequency": sales_frequency,
        "sales_proceeds_ratio": sales_proceeds / total_income if total_income > 0 else 0,
        "business_model_assessed": model,
        "measurement_basis": measurement,
        "reclassification_required": reclassification,
    }


@app.post("/ecl-general")
async def calculate_ecl_general(
    exposure_at_default: float,
    probability_of_default: float,
    loss_given_default: float,
    discount_rate: float,
    time_to_default_years: float,
):
    """Calculate ECL using general three-stage model."""
    if discount_rate > 0:
        discounted_ecl = (
            exposure_at_default
            * probability_of_default
            * loss_given_default
            / ((1 + discount_rate) ** time_to_default_years)
        )
    else:
        discounted_ecl = exposure_at_default * probability_of_default * loss_given_default

    return {
        "exposure_at_default": exposure_at_default,
        "probability_of_default": probability_of_default,
        "loss_given_default": loss_given_default,
        "undiscounted_ecl": exposure_at_default * probability_of_default * loss_given_default,
        "discount_rate": discount_rate,
        "time_to_default_years": time_to_default_years,
        "discounted_ecl": discounted_ecl,
    }


@app.post("/ecl-simplified")
async def calculate_ecl_simplified(
    trade_receivables_ageing: Dict[str, float],  # "current", "30_days", "60_days", "90_days", "120_plus"
    historical_loss_rates: Dict[str, float],
    forward_looking_adjustment: float,
):
    """Calculate ECL using simplified approach for trade receivables."""
    total_ecl = 0.0
    breakdown = {}

    for age_bucket, amount in trade_receivables_ageing.items():
        loss_rate = historical_loss_rates.get(age_bucket, 0.01)
        adjusted_rate = loss_rate * (1 + forward_looking_adjustment)
        ecl = amount * adjusted_rate
        breakdown[age_bucket] = {
            "amount": amount,
            "loss_rate": loss_rate,
            "forward_adjusted_rate": adjusted_rate,
            "ecl": ecl,
        }
        total_ecl += ecl

    return {
        "age_breakdown": breakdown,
        "total_trade_receivables": sum(trade_receivables_ageing.values()),
        "total_ecl": total_ecl,
        "ecl_coverage_ratio": (
            total_ecl / sum(trade_receivables_ageing.values()) if sum(trade_receivables_ageing.values()) > 0 else 0
        ),
    }


@app.post("/hedge-effectiveness")
async def assess_hedge_effectiveness(
    hedge_type: str,  # "fair_value", "cash_flow", "net_investment"
    hypothetical_derivative: float,
    actual_hedge_instrument: float,
    regression_correlation: float,
):
    """Assess hedge effectiveness under IFRS 9."""
    hedge_ratio = actual_hedge_instrument / hypothetical_derivative if hypothetical_derivative > 0 else 0
    effectiveness_score = regression_correlation

    qualitative_assessment = (
        "highly effective"
        if regression_correlation > 0.9
        else "effective" if regression_correlation > 0.8 else "not effective"
    )

    return {
        "hedge_type": hedge_type,
        "hedge_ratio": hedge_ratio,
        "regression_correlation": regression_correlation,
        "effectiveness_score": effectiveness_score,
        "qualitative_assessment": qualitative_assessment,
        "designated_as_hedge": regression_correlation > 0.8,
        "rebalancing_required": 0.8 < regression_correlation <= 0.9,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8149)
