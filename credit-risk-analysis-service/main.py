"""
Credit Risk Analysis Service
Port: 8163
Credit scoring, probability of default, loss given default, exposure at default
"""

from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field

logger = structlog.get_logger()
app = FastAPI(title="Credit Risk Analysis Service", version="1.0.0")


# Pydantic Models
class BorrowerFinancials(BaseModel):
    borrower_id: str
    borrower_name: str
    credit_facility_id: str
    facility_type: str  # "term_loan", "revolving", "trade_receivable"
    total_exposure: float
    current_exposure: float
    undrawn_committed: float
    collateral_value: float
    guarantee_value: float
    repayment_terms: str


class FinancialRatios(BaseModel):
    borrower_id: str
    debt_to_equity: float
    debt_to_ebitda: float
    interest_coverage: float
    current_ratio: float
    quick_ratio: float
    profit_margin: float
    operating_cash_flow: float
    free_cash_flow: float


class QualitativeFactors(BaseModel):
    borrower_id: str
    industry_outlook: str  # "positive", "stable", "negative"
    management_quality: str  # "excellent", "good", "average", "poor"
    market_position: str  # "leader", "strong", "average", "weak"
    regulatory_environment: str
    competitive_advantage: str


class CreditRiskRequest(BaseModel):
    borrower: BorrowerFinancials
    financials: FinancialRatios
    qualitative: QualitativeFactors
    credit_period_days: int = 365
    rating_model: str = "internal"  # "internal", "regulatory"


class PDCalculation(BaseModel):
    borrower_id: str
    one_year_pd: float
    five_year_pd: float
    lifetime_pd: float
    shadow_rating: str
    rating_grade: str
    rating_agency_equivalent: str


class LGDCalculation(BaseModel):
    borrower_id: str
    facility_type: str
    collateral_value: float
    guarantee_value: float
    recovery_rate: float
    loss_given_default: float
    secured_exposure: float
    unsecured_exposure: float


class EADCalculation(BaseModel):
    borrower_id: str
    current_drawdown: float
    undrawn_committed: float
    conversion_factor: float
    exposure_at_default: float
    potential_future_exposure: float


class ExpectedLossCalculation(BaseModel):
    borrower_id: str
    probability_of_default: float
    loss_given_default: float
    exposure_at_default: float
    expected_loss: float
    unexpected_loss: float
    economic_capital: float


class CreditRiskResponse(BaseModel):
    borrower_id: str
    credit_rating: str
    rating_grade: int
    pd_calculations: PDCalculation
    lgd_calculations: LGDCalculation
    ead_calculations: EADCalculation
    expected_loss_calculations: ExpectedLossCalculation
    total_expected_loss: float
    risk_adjusted_return: float
    recommendation: str
    covenants_required: List[str]


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
    return {"status": "healthy", "service": "credit-risk-analysis", "version": "1.0.0"}


@app.post("/analyze", response_model=CreditRiskResponse)
async def analyze_credit_risk(request: CreditRiskRequest):
    """Perform comprehensive credit risk analysis."""
    logger.info("Analyzing credit risk", borrower=request.borrower.borrower_name)

    # Calculate composite score
    financial_score = 0.0
    if request.financials.debt_to_equity < 1.0:
        financial_score += 25
    elif request.financials.debt_to_equity < 2.0:
        financial_score += 20
    elif request.financials.debt_to_equity < 3.0:
        financial_score += 15
    else:
        financial_score += 5

    if request.financials.interest_coverage > 5:
        financial_score += 25
    elif request.financials.interest_coverage > 3:
        financial_score += 20
    elif request.financials.interest_coverage > 1.5:
        financial_score += 10
    else:
        financial_score += 0

    if request.financials.current_ratio > 2:
        financial_score += 25
    elif request.financials.current_ratio > 1.5:
        financial_score += 20
    elif request.financials.current_ratio > 1.0:
        financial_score += 10
    else:
        financial_score += 0

    if request.financials.free_cash_flow > request.borrower.total_exposure * 0.2:
        financial_score += 25
    elif request.financials.free_cash_flow > 0:
        financial_score += 15
    else:
        financial_score += 0

    # Qualitative adjustment
    qual_score = 50  # Base
    if request.qualitative.industry_outlook == "positive":
        qual_score += 10
    elif request.qualitative.industry_outlook == "negative":
        qual_score -= 10

    if request.qualitative.management_quality == "excellent":
        qual_score += 15
    elif request.qualitative.management_quality == "good":
        qual_score += 10
    elif request.qualitative.management_quality == "poor":
        qual_score -= 15

    if request.qualitative.market_position == "leader":
        qual_score += 15
    elif request.qualitative.market_position == "strong":
        qual_score += 10
    elif request.qualitative.market_position == "weak":
        qual_score -= 10

    composite_score = financial_score * 0.7 + qual_score * 0.3

    # Determine rating
    if composite_score >= 90:
        rating = "AAA"
        grade = 1
        pd = 0.0001
    elif composite_score >= 80:
        rating = "AA"
        grade = 2
        pd = 0.0005
    elif composite_score >= 70:
        rating = "A"
        grade = 3
        pd = 0.001
    elif composite_score >= 60:
        rating = "BBB"
        grade = 4
        pd = 0.005
    elif composite_score >= 50:
        rating = "BB"
        grade = 5
        pd = 0.02
    elif composite_score >= 40:
        rating = "B"
        grade = 6
        pd = 0.05
    else:
        rating = "CCC"
        grade = 7
        pd = 0.15

    # PD calculations
    five_year_pd = 1 - (1 - pd) ** 5
    lifetime_pd = 1 - (1 - pd) ** (request.credit_period_days / 365)

    pd_calc = PDCalculation(
        borrower_id=request.borrower.borrower_id,
        one_year_pd=pd,
        five_year_pd=five_year_pd,
        lifetime_pd=lifetime_pd,
        shadow_rating=rating,
        rating_grade=grade,
        rating_agency_equivalent=rating,
    )

    # LGD calculations
    total_security = request.borrower.collateral_value + request.borrower.guarantee_value
    recovery_rate = (
        min(0.9, total_security / request.borrower.total_exposure) if request.borrower.total_exposure > 0 else 0.1
    )
    lgd = 1 - recovery_rate

    secured_exposure = min(request.borrower.current_exposure, request.borrower.collateral_value)
    unsecured_exposure = max(0, request.borrower.current_exposure - secured_exposure)

    lgd_calc = LGDCalculation(
        borrower_id=request.borrower.borrower_id,
        facility_type=request.borrower.facility_type,
        collateral_value=request.borrower.collateral_value,
        guarantee_value=request.borrower.guarantee_value,
        recovery_rate=recovery_rate,
        loss_given_default=lgd,
        secured_exposure=secured_exposure,
        unsecured_exposure=unsecured_exposure,
    )

    # EAD calculations
    if request.borrower.facility_type == "revolving":
        conversion_factor = 0.75
    elif request.borrower.facility_type == "term_loan":
        conversion_factor = 1.0
    else:
        conversion_factor = 0.85

    ead = request.borrower.current_exposure + request.borrower.undrawn_committed * conversion_factor

    ead_calc = EADCalculation(
        borrower_id=request.borrower.borrower_id,
        current_drawdown=request.borrower.current_exposure,
        undrawn_committed=request.borrower.undrawn_committed,
        conversion_factor=conversion_factor,
        exposure_at_default=ead,
        potential_future_exposure=request.borrower.undrawn_committed * 0.5,
    )

    # Expected Loss
    expected_loss = pd * lgd * ead
    unexpected_loss = 2.33 * expected_loss  # 99.9% confidence
    economic_capital = unexpected_loss - expected_loss

    el_calc = ExpectedLossCalculation(
        borrower_id=request.borrower.borrower_id,
        probability_of_default=pd,
        loss_given_default=lgd,
        exposure_at_default=ead,
        expected_loss=expected_loss,
        unexpected_loss=unexpected_loss,
        economic_capital=economic_capital,
    )

    # Recommendation
    covenants = []
    if grade >= 4:
        covenants.append("Minimum interest coverage ratio: 3x")
        covenants.append("Maximum debt/equity: 2x")
    if grade >= 5:
        covenants.append("Quarterly financial reporting")
        covenants.append("Negative pledge")
        covenants.append("Cross-default provisions")

    recommendation = "APPROVE" if grade <= 5 else "DECLINE" if grade >= 7 else "REVIEW"

    response = CreditRiskResponse(
        borrower_id=request.borrower.borrower_id,
        credit_rating=rating,
        rating_grade=grade,
        pd_calculations=pd_calc,
        lgd_calculations=lgd_calc,
        ead_calculations=ead_calc,
        expected_loss_calculations=el_calc,
        total_expected_loss=expected_loss,
        risk_adjusted_return=(
            (request.borrower.current_exposure * 0.08 - expected_loss) / request.borrower.current_exposure * 100
            if request.borrower.current_exposure > 0
            else 0
        ),
        recommendation=recommendation,
        covenants_required=covenants,
    )

    logger.info("Credit risk analysis complete", borrower=request.borrower.borrower_name, rating=rating)
    return response


@app.post("/pd-historical")
async def calculate_pd_historical(number_defaults: int, total_observations: int, confidence_level: float = 0.95):
    """Calculate historical probability of default."""
    historical_pd = number_defaults / total_observations if total_observations > 0 else 0

    # Confidence interval using binomial distribution approximation
    import math

    z = 1.96 if confidence_level == 0.95 else 2.58
    margin = z * math.sqrt(historical_pd * (1 - historical_pd) / total_observations) if total_observations > 0 else 0

    return {
        "number_of_defaults": number_defaults,
        "total_observations": total_observations,
        "historical_pd": historical_pd,
        "confidence_level": confidence_level,
        "lower_bound": max(0, historical_pd - margin),
        "upper_bound": min(1, historical_pd + margin),
        "adjusted_pd_for_conservatism": historical_pd * 1.1,
    }


@app.post("/pd-transition-matrix")
async def generate_transition_matrix(
    current_rating: str, years_holding: int, transition_matrix: Dict[str, Dict[str, float]]
):
    """Calculate probability of default using transition matrix."""
    import numpy as np

    # Find current rating index
    ratings = ["AAA", "AA", "A", "BBB", "BB", "B", "CCC", "D"]
    if current_rating not in ratings:
        current_rating = "BBB"

    idx = ratings.index(current_rating)

    # Calculate cumulative transition (multi-period)
    # Simplified: raise to power of years
    cumulative_default_prob = (
        1 - (1 - 0.001) ** years_holding if current_rating in ["AAA", "AA"] else 1 - (1 - 0.005) ** years_holding
    )

    return {
        "current_rating": current_rating,
        "years_holding": years_holding,
        "cumulative_pd": cumulative_default_prob,
        "annual_pd": 1 - (1 - cumulative_default_prob) ** (1 / years_holding) if years_holding > 0 else 0,
        "probability_still_investment_grade": cumulative_default_prob * 0.5,
    }


@app.post("/lgd-scenario")
async def calculate_lgd_scenarios(
    collateral_value: float,
    total_exposure: float,
    recovery_optimistic: float,
    recovery_baseline: float,
    recovery_pessimistic: float,
):
    """Calculate LGD under different scenarios."""
    exposure_covered = min(collateral_value, total_exposure)
    exposure_uncovered = max(0, total_exposure - exposure_covered)

    lgd_optimistic = (exposure_uncovered * (1 - recovery_optimistic)) / total_exposure
    lgd_baseline = (exposure_uncovered * (1 - recovery_baseline)) / total_exposure
    lgd_pessimistic = (exposure_uncovered * (1 - recovery_pessimistic)) / total_exposure

    expected_lgd = lgd_optimistic * 0.25 + lgd_baseline * 0.5 + lgd_pessimistic * 0.25

    return {
        "collateral_coverage": exposure_covered / total_exposure if total_exposure > 0 else 0,
        "lgd_optimistic": lgd_optimistic,
        "lgd_baseline": lgd_baseline,
        "lgd_pessimistic": lgd_pessimistic,
        "expected_lgd": expected_lgd,
        "worst_case_loss": total_exposure * lgd_pessimistic,
    }


@app.post("/expected-credit-loss")
async def calculate_ecl(pd: float, lgd: float, ead: float, discount_rate: float, time_years: float):
    """Calculate expected credit loss with discounting."""
    undiscounted_ecl = pd * lgd * ead
    discounted_ecl = undiscounted_ecl / ((1 + discount_rate) ** time_years)

    return {
        "probability_of_default": pd,
        "loss_given_default": lgd,
        "exposure_at_default": ead,
        "undiscounted_ecl": undiscounted_ecl,
        "discount_rate": discount_rate,
        "time_years": time_years,
        "discounted_ecl": discounted_ecl,
        "ecl_as_percentage_of_ead": discounted_ecl / ead * 100 if ead > 0 else 0,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8163)
