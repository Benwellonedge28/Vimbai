//! Credit risk: composite rating, PD, LGD, EAD and expected loss.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::money::{money, round_dp};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Borrower {
    pub borrower_id: String,
    pub borrower_name: String,
    pub facility_type: String,
    pub total_exposure: f64,
    pub current_exposure: f64,
    pub undrawn_committed: f64,
    pub collateral_value: f64,
    pub guarantee_value: f64,
}

impl Borrower {
    pub fn new(
        borrower_id: &str,
        name: &str,
        facility_type: &str,
        total_exposure: f64,
    ) -> Borrower {
        Borrower {
            borrower_id: borrower_id.to_string(),
            borrower_name: name.to_string(),
            facility_type: facility_type.to_string(),
            total_exposure,
            current_exposure: total_exposure,
            undrawn_committed: 0.0,
            collateral_value: 0.0,
            guarantee_value: 0.0,
        }
    }

    pub fn drawn(mut self, current: f64, undrawn: f64) -> Borrower {
        self.current_exposure = current;
        self.undrawn_committed = undrawn;
        self
    }

    pub fn secured_by(mut self, collateral: f64, guarantee: f64) -> Borrower {
        self.collateral_value = collateral;
        self.guarantee_value = guarantee;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Financials {
    pub debt_to_equity: f64,
    pub interest_coverage: f64,
    pub current_ratio: f64,
    pub free_cash_flow: f64,
}

impl Financials {
    pub fn new(
        debt_to_equity: f64,
        interest_coverage: f64,
        current_ratio: f64,
        free_cash_flow: f64,
    ) -> Financials {
        Financials {
            debt_to_equity,
            interest_coverage,
            current_ratio,
            free_cash_flow,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Qualitative {
    /// `positive`, `neutral`, `negative`.
    pub industry_outlook: String,
    /// `excellent`, `good`, `fair`, `poor`.
    pub management_quality: String,
    /// `leader`, `strong`, `average`, `weak`.
    pub market_position: String,
}

impl Default for Qualitative {
    fn default() -> Self {
        Qualitative {
            industry_outlook: "neutral".to_string(),
            management_quality: "fair".to_string(),
            market_position: "average".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreditRequest {
    pub borrower: Borrower,
    pub financials: Financials,
    pub qualitative: Qualitative,
    pub credit_period_days: f64,
}

impl CreditRequest {
    pub fn new(borrower: Borrower, financials: Financials) -> CreditRequest {
        CreditRequest {
            borrower,
            financials,
            qualitative: Qualitative::default(),
            credit_period_days: 365.0,
        }
    }

    pub fn with_qualitative(mut self, qualitative: Qualitative) -> CreditRequest {
        self.qualitative = qualitative;
        self
    }

    pub fn over_days(mut self, days: f64) -> CreditRequest {
        self.credit_period_days = days;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CreditRiskResponse {
    pub borrower_id: String,
    pub credit_rating: String,
    pub rating_grade: u8,
    pub pd_calculations: Value,
    pub lgd_calculations: Value,
    pub ead_calculations: Value,
    pub expected_loss_calculations: Value,
    pub total_expected_loss: f64,
    pub risk_adjusted_return: f64,
    pub recommendation: String,
    pub covenants_required: Vec<String>,
}

/// Financial score, out of 100.
///
/// Preserved: each of the four measures contributes at most 25, and the free cash
/// flow test compares against 20% of *total* exposure rather than the drawn
/// amount, so a large undrawn commitment makes the fourth quarter harder to earn.
pub fn financial_score(financials: &Financials, total_exposure: f64) -> f64 {
    let mut score = 0.0;
    score += if financials.debt_to_equity < 1.0 {
        25.0
    } else if financials.debt_to_equity < 2.0 {
        20.0
    } else if financials.debt_to_equity < 3.0 {
        15.0
    } else {
        5.0
    };
    score += if financials.interest_coverage > 5.0 {
        25.0
    } else if financials.interest_coverage > 3.0 {
        20.0
    } else if financials.interest_coverage > 1.5 {
        10.0
    } else {
        0.0
    };
    score += if financials.current_ratio > 2.0 {
        25.0
    } else if financials.current_ratio > 1.5 {
        20.0
    } else if financials.current_ratio > 1.0 {
        10.0
    } else {
        0.0
    };
    score += if financials.free_cash_flow > total_exposure * 0.2 {
        25.0
    } else if financials.free_cash_flow > 0.0 {
        15.0
    } else {
        0.0
    };
    score
}

fn qualitative_score(qualitative: &Qualitative) -> f64 {
    let mut score = 50.0;
    score += match qualitative.industry_outlook.as_str() {
        "positive" => 10.0,
        "negative" => -10.0,
        _ => 0.0,
    };
    score += match qualitative.management_quality.as_str() {
        "excellent" => 15.0,
        "good" => 10.0,
        "poor" => -15.0,
        _ => 0.0,
    };
    score += match qualitative.market_position.as_str() {
        "leader" => 15.0,
        "strong" => 10.0,
        "weak" => -10.0,
        _ => 0.0,
    };
    score
}

/// Rating band, grade and one-year PD for a composite score.
pub fn rate(composite: f64) -> (&'static str, u8, f64) {
    if composite >= 90.0 {
        ("AAA", 1, 0.0001)
    } else if composite >= 80.0 {
        ("AA", 2, 0.0005)
    } else if composite >= 70.0 {
        ("A", 3, 0.001)
    } else if composite >= 60.0 {
        ("BBB", 4, 0.005)
    } else if composite >= 50.0 {
        ("BB", 5, 0.02)
    } else if composite >= 40.0 {
        ("B", 6, 0.05)
    } else {
        ("CCC", 7, 0.15)
    }
}

/// Full analysis.
///
/// Preserved quirks: recovery is capped at 90% but has a floor of 10% only in the
/// zero-exposure case, so an unsecured loan recovers nothing; and the risk
/// adjusted return assumes an 8% margin on the drawn balance, which is a
/// placeholder pricing assumption rather than a facility rate.
pub fn analyze(request: &CreditRequest) -> CreditRiskResponse {
    let borrower = &request.borrower;
    let financial = financial_score(&request.financials, borrower.total_exposure);
    let qualitative = qualitative_score(&request.qualitative);
    let composite = financial * 0.7 + qualitative * 0.3;

    let (rating, grade, pd) = rate(composite);

    let five_year_pd = 1.0 - (1.0 - pd).powi(5);
    let lifetime_pd = 1.0 - (1.0 - pd).powf(request.credit_period_days / 365.0);

    let total_security = borrower.collateral_value + borrower.guarantee_value;
    let recovery_rate = if borrower.total_exposure > 0.0 {
        (total_security / borrower.total_exposure).min(0.9)
    } else {
        0.1
    };
    let lgd = 1.0 - recovery_rate;
    let secured_exposure = borrower.current_exposure.min(borrower.collateral_value);
    let unsecured_exposure = (borrower.current_exposure - secured_exposure).max(0.0);

    let conversion_factor = match borrower.facility_type.as_str() {
        "revolving" => 0.75,
        "term_loan" => 1.0,
        _ => 0.85,
    };
    let ead = borrower.current_exposure + borrower.undrawn_committed * conversion_factor;

    let expected_loss = pd * lgd * ead;
    let unexpected_loss = 2.33 * expected_loss;
    let economic_capital = unexpected_loss - expected_loss;

    let mut covenants = Vec::new();
    if grade >= 4 {
        covenants.push("Minimum interest coverage ratio: 3x".to_string());
        covenants.push("Maximum debt/equity: 2x".to_string());
    }
    if grade >= 5 {
        covenants.push("Quarterly financial reporting".to_string());
        covenants.push("Negative pledge".to_string());
        covenants.push("Cross-default provisions".to_string());
    }
    let recommendation = if grade <= 5 {
        "APPROVE"
    } else if grade >= 7 {
        "DECLINE"
    } else {
        "REVIEW"
    };

    let raroc = if borrower.current_exposure > 0.0 {
        (borrower.current_exposure * 0.08 - expected_loss) / borrower.current_exposure * 100.0
    } else {
        0.0
    };

    CreditRiskResponse {
        borrower_id: borrower.borrower_id.clone(),
        credit_rating: rating.to_string(),
        rating_grade: grade,
        pd_calculations: json!({
            "borrower_id": borrower.borrower_id,
            "one_year_pd": pd,
            "five_year_pd": five_year_pd,
            "lifetime_pd": lifetime_pd,
            "shadow_rating": rating,
            "rating_grade": grade,
            "rating_agency_equivalent": rating,
        }),
        lgd_calculations: json!({
            "borrower_id": borrower.borrower_id,
            "facility_type": borrower.facility_type,
            "collateral_value": borrower.collateral_value,
            "guarantee_value": borrower.guarantee_value,
            "recovery_rate": recovery_rate,
            "loss_given_default": lgd,
            "secured_exposure": secured_exposure,
            "unsecured_exposure": unsecured_exposure,
        }),
        ead_calculations: json!({
            "borrower_id": borrower.borrower_id,
            "current_drawdown": borrower.current_exposure,
            "undrawn_committed": borrower.undrawn_committed,
            "conversion_factor": conversion_factor,
            "exposure_at_default": ead,
            "potential_future_exposure": borrower.undrawn_committed * 0.5,
        }),
        expected_loss_calculations: json!({
            "borrower_id": borrower.borrower_id,
            "probability_of_default": pd,
            "loss_given_default": lgd,
            "exposure_at_default": ead,
            "expected_loss": expected_loss,
            "unexpected_loss": unexpected_loss,
            "economic_capital": economic_capital,
        }),
        total_expected_loss: money(expected_loss),
        risk_adjusted_return: round_dp(raroc, 2),
        recommendation: recommendation.to_string(),
        covenants_required: covenants,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HistoricalPd {
    pub number_of_defaults: i64,
    pub total_observations: i64,
    pub historical_pd: f64,
    pub confidence_level: f64,
    pub lower_bound: f64,
    pub upper_bound: f64,
    pub adjusted_pd_for_conservatism: f64,
}

/// Historical PD with a binomial interval.
///
/// Preserved: the confidence-level choice is a binary 1.96 for anything treated as
/// 95% and 2.58 otherwise, so 90% and 99.9% both report a 99% interval.
pub fn historical_pd(
    number_defaults: i64,
    total_observations: i64,
    confidence_level: f64,
) -> HistoricalPd {
    if total_observations <= 0 {
        return HistoricalPd {
            number_of_defaults: number_defaults,
            total_observations,
            historical_pd: 0.0,
            confidence_level,
            lower_bound: 0.0,
            upper_bound: 0.0,
            adjusted_pd_for_conservatism: 0.0,
        };
    }
    let pd = number_defaults as f64 / total_observations as f64;
    let z = if (confidence_level - 0.95).abs() < 1e-12 {
        1.96
    } else {
        2.58
    };
    let margin = z * (pd * (1.0 - pd) / total_observations as f64).sqrt();
    HistoricalPd {
        number_of_defaults: number_defaults,
        total_observations,
        historical_pd: pd,
        confidence_level,
        lower_bound: (pd - margin).max(0.0),
        upper_bound: (pd + margin).min(1.0),
        adjusted_pd_for_conservatism: pd * 1.1,
    }
}
