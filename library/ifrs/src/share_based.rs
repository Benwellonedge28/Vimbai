//! IFRS 2 share-based payment: grant expense, cash-settled liability,
//! dilutive effect, instrument fair value, performance conditions,
//! modifications and group transactions.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VestingCondition {
    pub condition_id: String,
    /// "service", "performance" or "market". Market conditions are skipped
    /// entirely by the vesting maths.
    pub condition_type: String,
    pub description: String,
    pub target_value: f64,
    pub probability_assessment: f64,
    pub probability_weight: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShareOption {
    pub option_id: String,
    pub grant_date: String,
    pub number_of_options: i64,
    pub exercise_price: f64,
    pub fair_value_per_option: f64,
    pub vesting_period_years: i64,
    pub expiry_date: String,
    pub vesting_conditions: Vec<VestingCondition>,
    pub employee_category: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShareBasedPaymentRequest {
    pub company_id: String,
    pub reporting_date: String,
    pub equity_settled_grants: Vec<ShareOption>,
    pub cash_settled_grants: Vec<ShareOption>,
    pub shares_issued: i64,
    pub share_price_at_reporting: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VestingCalculation {
    pub option_id: String,
    pub grant_date: String,
    pub options_granted: i64,
    pub options_expected_to_vest: i64,
    pub forfeiture_rate: f64,
    pub cumulative_expense: f64,
    pub period_expense: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ifrs2Response {
    pub company_id: String,
    pub reporting_date: String,
    pub equity_settled_expense: f64,
    pub cash_settled_expense: f64,
    pub total_expense: f64,
    pub equity_settled_vesting: Vec<VestingCalculation>,
    pub cash_settled_vesting: Vec<VestingCalculation>,
    pub liability_for_cash_settled: f64,
    pub equity_reserve: f64,
    pub dilutive_effect: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShareBasedError {
    /// `fair_value_per_option / vesting_period_years` needs a positive term.
    ZeroVestingPeriod { option_id: String },
    /// The instrument pricing divides by volatility and the square root of term.
    InvalidPricingInput { field: &'static str },
}

impl std::fmt::Display for ShareBasedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShareBasedError::ZeroVestingPeriod { option_id } => {
                write!(
                    f,
                    "vesting_period_years must be positive for grant {option_id}"
                )
            }
            ShareBasedError::InvalidPricingInput { field } => {
                write!(
                    f,
                    "{field} must be greater than zero to price the instrument"
                )
            }
        }
    }
}

impl std::error::Error for ShareBasedError {}

/// `/calculate`.
///
/// PYTHON QUIRKS, all preserved:
/// - equity grants multiply the option count by every non-market condition's
///   probability and start from a 5% base forfeiture, adding
///   `1 - probability_weight` per condition. Only the first 30 points of that
///   forfeiture rate reduce the count, but the raw figure is still reported,
///   so it can exceed 100%;
/// - cumulative expense is always TWO periods' worth, whatever the vesting
///   period, so a ten-year grant reports two years of charge;
/// - cash-settled grants take no forfeiture haircut at all and report their
///   forfeiture rate as zero, while their "cumulative expense" is an
///   intrinsic-value liability at 50% vested that ignores the fair value
///   used for the period charge;
/// - the average strike price is weighted over EQUITY grants but divided by
///   the count of ALL grants, so cash-settled options drag the strike down,
///   and the dilutive figure is truncated to a whole number.
pub fn calculate_share_based_payment(
    req: &ShareBasedPaymentRequest,
) -> Result<Ifrs2Response, ShareBasedError> {
    let mut equity_vesting = Vec::new();
    let mut cash_vesting = Vec::new();
    let mut total_equity_expense = 0.0;
    let mut total_cash_expense = 0.0;

    for option in &req.equity_settled_grants {
        if option.vesting_period_years <= 0 {
            return Err(ShareBasedError::ZeroVestingPeriod {
                option_id: option.option_id.clone(),
            });
        }
        let mut expected_vesting = option.number_of_options as f64;
        let mut forfeiture_rate = 0.05;
        for condition in &option.vesting_conditions {
            if condition.condition_type != "market" {
                expected_vesting *= condition.probability_assessment;
                forfeiture_rate += 1.0 - condition.probability_weight;
            }
        }
        let options_to_expense =
            (expected_vesting * (1.0 - forfeiture_rate.min(0.3))).trunc() as i64;
        let per_year_expense = options_to_expense as f64 * option.fair_value_per_option
            / option.vesting_period_years as f64;
        let cumulative_expense = per_year_expense * 2.0;

        equity_vesting.push(VestingCalculation {
            option_id: option.option_id.clone(),
            grant_date: option.grant_date.clone(),
            options_granted: option.number_of_options,
            options_expected_to_vest: options_to_expense,
            forfeiture_rate,
            cumulative_expense,
            period_expense: per_year_expense,
        });
        total_equity_expense += per_year_expense;
    }

    for option in &req.cash_settled_grants {
        if option.vesting_period_years <= 0 {
            return Err(ShareBasedError::ZeroVestingPeriod {
                option_id: option.option_id.clone(),
            });
        }
        let mut expected_vesting = option.number_of_options as f64;
        for condition in &option.vesting_conditions {
            if condition.condition_type != "market" {
                expected_vesting *= condition.probability_assessment;
            }
        }
        let options_to_expense = expected_vesting.trunc() as i64;
        let current_intrinsic = (req.share_price_at_reporting - option.exercise_price).max(0.0);
        let liability = current_intrinsic * options_to_expense as f64 * 0.5;
        let per_year_expense = options_to_expense as f64 * option.fair_value_per_option
            / option.vesting_period_years as f64;

        cash_vesting.push(VestingCalculation {
            option_id: option.option_id.clone(),
            grant_date: option.grant_date.clone(),
            options_granted: option.number_of_options,
            options_expected_to_vest: options_to_expense,
            forfeiture_rate: 0.0,
            cumulative_expense: liability,
            period_expense: per_year_expense,
        });
        total_cash_expense += per_year_expense;
    }

    let total_options: i64 = req
        .equity_settled_grants
        .iter()
        .chain(req.cash_settled_grants.iter())
        .map(|o| o.number_of_options)
        .sum();
    let avg_price = req.share_price_at_reporting;
    let exercise_price = if total_options > 0 {
        let weighted: f64 = req
            .equity_settled_grants
            .iter()
            .map(|o| o.exercise_price * o.number_of_options as f64)
            .sum();
        weighted / total_options as f64
    } else {
        0.0
    };
    let dilutive = if avg_price > exercise_price {
        total_options as f64 * (1.0 - exercise_price / avg_price)
    } else {
        0.0
    };

    Ok(Ifrs2Response {
        company_id: req.company_id.clone(),
        reporting_date: req.reporting_date.clone(),
        equity_settled_expense: total_equity_expense,
        cash_settled_expense: total_cash_expense,
        total_expense: total_equity_expense + total_cash_expense,
        liability_for_cash_settled: cash_vesting.iter().map(|v| v.cumulative_expense).sum(),
        equity_reserve: equity_vesting.iter().map(|v| v.cumulative_expense).sum(),
        dilutive_effect: dilutive.trunc() as i64,
        equity_settled_vesting: equity_vesting,
        cash_settled_vesting: cash_vesting,
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstrumentFairValue {
    pub option_type: String,
    pub spot_price: f64,
    pub exercise_price: f64,
    pub time_to_expiry: f64,
    pub risk_free_rate: f64,
    pub volatility: f64,
    pub dividend_yield: f64,
    pub d1: f64,
    pub d2: f64,
    pub calculated_fair_value: f64,
    pub adjustment_applied: f64,
}

/// `/fair-value-equity-instruments`.
///
/// PYTHON QUIRK, preserved verbatim: this is NOT Black-Scholes. Only `d1`
/// and `d2` are computed as usual; the "call" is
/// `S * e^(-qT) * 0.5 * (1 + e^d1 + e^(-d1))`, which ignores the strike,
/// the rate and both normal-distribution terms and grows without bound, and
/// the put value that IS computed is then discarded. Fair value is that
/// figure times a flat type factor: lookback 1.2, barrier 0.8, otherwise
/// 1.0. Reimplemented as it stands for parity; it should not be used to
/// value a real grant.
///
/// Python raises on a zero or negative spot, strike, term or volatility;
/// this returns `ShareBasedError::InvalidPricingInput`.
pub fn calculate_instrument_fair_value(
    option_type: &str,
    spot_price: f64,
    exercise_price: f64,
    time_to_expiry: f64,
    risk_free_rate: f64,
    volatility: f64,
    dividend_yield: f64,
) -> Result<InstrumentFairValue, ShareBasedError> {
    if spot_price <= 0.0 {
        return Err(ShareBasedError::InvalidPricingInput {
            field: "spot_price",
        });
    }
    if exercise_price <= 0.0 {
        return Err(ShareBasedError::InvalidPricingInput {
            field: "exercise_price",
        });
    }
    if time_to_expiry <= 0.0 {
        return Err(ShareBasedError::InvalidPricingInput {
            field: "time_to_expiry",
        });
    }
    if volatility <= 0.0 {
        return Err(ShareBasedError::InvalidPricingInput {
            field: "volatility",
        });
    }

    let root_t = time_to_expiry.sqrt();
    let d1 = ((spot_price / exercise_price).ln()
        + (risk_free_rate + volatility * volatility / 2.0) * time_to_expiry)
        / (volatility * root_t);
    let d2 = d1 - volatility * root_t;

    let call_value = spot_price
        * (-dividend_yield * time_to_expiry).exp()
        * 0.5
        * (1.0 + d1.exp() + (-d1).exp());

    let adjustment_factor = match option_type {
        "lookback" => 1.2,
        "barrier" => 0.8,
        _ => 1.0,
    };

    Ok(InstrumentFairValue {
        option_type: option_type.to_string(),
        spot_price,
        exercise_price,
        time_to_expiry,
        risk_free_rate,
        volatility,
        dividend_yield,
        d1,
        d2,
        calculated_fair_value: call_value * adjustment_factor,
        adjustment_applied: adjustment_factor,
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceCondition {
    pub condition_type: String,
    pub target_metric: f64,
    pub current_metric: f64,
    pub achievement_rate: f64,
    pub measurement_period: f64,
    pub expected_achievement_probability: f64,
    pub expense_multiplier: f64,
}

/// `/performance-condition`.
///
/// PYTHON QUIRKS, preserved: the remaining time is derived as
/// `1 - measurement_period / 3` against a hard-wired three-year assumption,
/// so any period longer than three years yields a NEGATIVE remaining share;
/// EPS growth is credited with a tenth of that share and capped at 1; TSR
/// simply returns the supplied regression probability; everything else is
/// the raw achievement rate capped at 1 with no floor, so an under-target
/// metric passes straight through as a sub-1 multiplier.
pub fn assess_performance_condition(
    condition_type: &str,
    target_metric: f64,
    current_metric: f64,
    measurement_period: f64,
    probability_based_on_regression: f64,
) -> PerformanceCondition {
    let achievement_rate = if target_metric > 0.0 {
        current_metric / target_metric
    } else {
        0.0
    };
    let time_remaining_pct = 1.0 - (measurement_period / 3.0);

    let expected_achievement = match condition_type {
        "EPS_growth" => (achievement_rate * (1.0 + time_remaining_pct * 0.1)).min(1.0),
        "TSR" => probability_based_on_regression,
        _ => achievement_rate.min(1.0),
    };

    PerformanceCondition {
        condition_type: condition_type.to_string(),
        target_metric,
        current_metric,
        achievement_rate,
        measurement_period,
        expected_achievement_probability: expected_achievement,
        expense_multiplier: expected_achievement,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GrantModification {
    pub original_grant_date: String,
    pub modification_date: String,
    pub original_fair_value: f64,
    pub new_fair_value: f64,
    pub incremental_fair_value: f64,
    pub original_options: i64,
    pub new_options: i64,
    pub years_elapsed: i64,
    pub remaining_vesting_period: i64,
    pub catch_up_expense: f64,
    pub prospective_expense: f64,
    pub total_modification_impact: f64,
}

/// `/modification` (named `account_for_grant_modification` to avoid
/// clashing with the IFRS 15 contract modification).
///
/// PYTHON QUIRKS, all preserved:
/// - a fall in fair value is floored at zero, so a modification can never
///   reduce expense;
/// - the remaining period is floored at one year;
/// - the new per-period figure divides the combined value by the FULL new
///   vesting period, not the remaining one;
/// - the catch-up is floored at zero, so a grant already expensed ahead of
///   the recalculated figure is never credited back;
/// - `new_options` is carried through and never enters any calculation, so
///   changing the number of instruments in a modification has no effect on
///   the expense.
#[allow(clippy::too_many_arguments)] // mirrors the FastAPI signature 1:1
pub fn account_for_grant_modification(
    original_grant_date: &str,
    original_fair_value: f64,
    original_options: i64,
    modification_date: &str,
    new_fair_value: f64,
    new_options: i64,
    new_vesting_period: i64,
    years_elapsed: i64,
    cumulative_expense_original: f64,
) -> GrantModification {
    let mut incremental_value = new_fair_value - original_fair_value;
    if incremental_value < 0.0 {
        incremental_value = 0.0;
    }
    let remaining_period = (new_vesting_period - years_elapsed).max(1);
    let total_value = original_fair_value + incremental_value;
    let per_period_new = total_value / new_vesting_period as f64;
    let per_period_incremental = incremental_value / remaining_period as f64;
    let modified_cumulative = per_period_new * years_elapsed as f64;
    let catch_up_expense = (modified_cumulative - cumulative_expense_original).max(0.0);

    GrantModification {
        original_grant_date: original_grant_date.to_string(),
        modification_date: modification_date.to_string(),
        original_fair_value,
        new_fair_value,
        incremental_fair_value: incremental_value,
        original_options,
        new_options,
        years_elapsed,
        remaining_vesting_period: remaining_period,
        catch_up_expense,
        prospective_expense: per_period_incremental,
        total_modification_impact: catch_up_expense + per_period_incremental,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GroupTransaction {
    pub transaction_type: String,
    pub parent_equity_settled: f64,
    pub parent_cash_settled: f64,
    pub non_controlling_interests: f64,
    pub nci_impact: f64,
    pub treatment: String,
    pub financial_statement_impact: String,
}

/// `/group-transaction`. Anything other than "equity_for_equity" is treated
/// as cash-settled and assigns a flat 10% of the NCI figure.
pub fn account_for_group_transaction(
    parent_equity_settled: f64,
    parent_cash_settled: f64,
    non_controlling_interests: f64,
    transaction_type: &str,
) -> GroupTransaction {
    let equity = transaction_type == "equity_for_equity";
    GroupTransaction {
        transaction_type: transaction_type.to_string(),
        parent_equity_settled,
        parent_cash_settled,
        non_controlling_interests,
        nci_impact: if equity {
            0.0
        } else {
            non_controlling_interests * 0.1
        },
        treatment: if equity {
            "equity-settled - no change in accounting".to_string()
        } else {
            "cash-settled - remeasurement required".to_string()
        },
        financial_statement_impact: if equity {
            "equity reserve".to_string()
        } else {
            "liability".to_string()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn condition(kind: &str, prob: f64, weight: f64) -> VestingCondition {
        VestingCondition {
            condition_id: "vc".into(),
            condition_type: kind.into(),
            description: format!("{kind} condition"),
            target_value: 1.0,
            probability_assessment: prob,
            probability_weight: weight,
        }
    }

    fn option(id: &str, options: i64, strike: f64, fv: f64, years: i64) -> ShareOption {
        ShareOption {
            option_id: id.into(),
            grant_date: "2024-01-01".into(),
            number_of_options: options,
            exercise_price: strike,
            fair_value_per_option: fv,
            vesting_period_years: years,
            expiry_date: "2034-01-01".into(),
            vesting_conditions: Vec::new(),
            employee_category: "executive".into(),
        }
    }

    fn request() -> ShareBasedPaymentRequest {
        ShareBasedPaymentRequest {
            company_id: "c1".into(),
            reporting_date: "2026-12-31".into(),
            equity_settled_grants: vec![ShareOption {
                vesting_conditions: vec![condition("service", 0.9, 0.9)],
                ..option("E1", 10_000, 5.0, 2.0, 4)
            }],
            cash_settled_grants: vec![ShareOption {
                vesting_conditions: vec![condition("performance", 0.8, 0.8)],
                ..option("C1", 5_000, 4.0, 1.5, 3)
            }],
            shares_issued: 1_000_000,
            share_price_at_reporting: 10.0,
        }
    }

    #[test]
    fn equity_grant_applies_probability_and_capped_forfeiture() {
        let r = calculate_share_based_payment(&request()).unwrap();
        let e = &r.equity_settled_vesting[0];
        // 10,000 * 0.9 = 9,000 expected; forfeiture 0.05 + (1 - 0.9) = 0.15
        assert!((e.forfeiture_rate - 0.15).abs() < 1e-12);
        assert_eq!(e.options_expected_to_vest, 7_650); // 9,000 * 0.85
        assert_eq!(e.period_expense, 7_650.0 * 2.0 / 4.0);
        // cumulative is always two periods, whatever the vesting term
        assert_eq!(e.cumulative_expense, e.period_expense * 2.0);
    }

    #[test]
    fn forfeiture_reported_raw_but_only_thirty_points_apply() {
        let heavy = calculate_share_based_payment(&ShareBasedPaymentRequest {
            equity_settled_grants: vec![ShareOption {
                vesting_conditions: vec![
                    condition("service", 1.0, 0.0),
                    condition("service", 1.0, 0.0),
                ],
                ..option("E1", 1_000, 5.0, 2.0, 2)
            }],
            cash_settled_grants: vec![],
            share_price_at_reporting: 10.0,
            ..Default::default()
        })
        .unwrap();
        let e = &heavy.equity_settled_vesting[0];
        // 0.05 + 1 + 1 = 2.05 reported, haircut capped at 0.30
        assert!((e.forfeiture_rate - 2.05).abs() < 1e-12);
        assert_eq!(e.options_expected_to_vest, 700);
    }

    #[test]
    fn market_conditions_are_skipped() {
        let r = calculate_share_based_payment(&ShareBasedPaymentRequest {
            equity_settled_grants: vec![ShareOption {
                vesting_conditions: vec![
                    condition("service", 0.9, 0.9),
                    condition("market", 0.1, 0.1),
                ],
                ..option("E1", 10_000, 5.0, 2.0, 4)
            }],
            cash_settled_grants: vec![],
            share_price_at_reporting: 10.0,
            ..Default::default()
        })
        .unwrap();
        let e = &r.equity_settled_vesting[0];
        assert!((e.forfeiture_rate - 0.15).abs() < 1e-12);
        assert_eq!(e.options_expected_to_vest, 7_650);
    }

    #[test]
    fn cash_grant_takes_no_forfeiture_and_holds_a_half_vested_liability() {
        let r = calculate_share_based_payment(&request()).unwrap();
        let c = &r.cash_settled_vesting[0];
        // 5,000 * 0.8 = 4,000 expected, no haircut
        assert_eq!(c.options_expected_to_vest, 4_000);
        assert_eq!(c.forfeiture_rate, 0.0);
        // intrinsic (10 - 4) * 4,000 * 50% vested = 12,000
        assert_eq!(c.cumulative_expense, 12_000.0);
        // but the period charge uses the FAIR VALUE, not the intrinsic
        assert_eq!(c.period_expense, 4_000.0 * 1.5 / 3.0);
        assert_eq!(r.liability_for_cash_settled, 12_000.0);
    }

    #[test]
    fn totals_and_equity_reserve() {
        let r = calculate_share_based_payment(&request()).unwrap();
        assert_eq!(r.equity_settled_expense, 7_650.0 * 2.0 / 4.0);
        assert_eq!(r.cash_settled_expense, 2_000.0);
        assert_eq!(
            r.total_expense,
            r.equity_settled_expense + r.cash_settled_expense
        );
        assert_eq!(r.equity_reserve, 7_650.0);
    }

    #[test]
    fn dilutive_effect_uses_equity_strike_over_all_options() {
        let r = calculate_share_based_payment(&request()).unwrap();
        // strike weighted over equity only: 5 * 10,000 = 50,000 / 15,000 = 3.3333
        let strike: f64 = 50_000.0 / 15_000.0;
        let expected = (15_000.0 * (1.0 - strike / 10.0)).trunc();
        assert_eq!(r.dilutive_effect as f64, expected);
        // cash-settled options drag the strike down, inflating dilution
        assert!(strike < 5.0);
    }

    #[test]
    fn no_dilution_when_strike_exceeds_market() {
        let r = calculate_share_based_payment(&ShareBasedPaymentRequest {
            equity_settled_grants: vec![option("E1", 1_000, 20.0, 2.0, 3)],
            cash_settled_grants: vec![],
            share_price_at_reporting: 10.0,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(r.dilutive_effect, 0);
    }

    #[test]
    fn zero_vesting_period_is_a_typed_error() {
        let err = calculate_share_based_payment(&ShareBasedPaymentRequest {
            equity_settled_grants: vec![option("E1", 1_000, 5.0, 2.0, 0)],
            ..Default::default()
        })
        .unwrap_err();
        assert_eq!(
            err,
            ShareBasedError::ZeroVestingPeriod {
                option_id: "E1".into()
            }
        );
    }

    #[test]
    fn instrument_value_is_not_a_black_scholes_call() {
        let v = calculate_instrument_fair_value("plain_vanilla", 100.0, 90.0, 2.0, 0.05, 0.25, 0.0)
            .unwrap();
        let expected = 100.0 * 0.5 * (1.0 + v.d1.exp() + (-v.d1).exp());
        assert!((v.calculated_fair_value - expected).abs() < 1e-9);
        // far above any sane option value, because strike and rate are ignored
        assert!(v.calculated_fair_value > 100.0);
        assert_eq!(v.adjustment_applied, 1.0);
        let check = (100.0f64 / 90.0).ln() + (0.05 + 0.25f64.powi(2) / 2.0) * 2.0;
        assert!((v.d1 - check / (0.25 * 2.0f64.sqrt())).abs() < 1e-12);
        assert!((v.d2 - (v.d1 - 0.25 * 2.0f64.sqrt())).abs() < 1e-12);
    }

    #[test]
    fn instrument_type_factors() {
        let base =
            calculate_instrument_fair_value("ESOP", 100.0, 100.0, 1.0, 0.04, 0.2, 0.01).unwrap();
        let look = calculate_instrument_fair_value("lookback", 100.0, 100.0, 1.0, 0.04, 0.2, 0.01)
            .unwrap();
        let barrier =
            calculate_instrument_fair_value("barrier", 100.0, 100.0, 1.0, 0.04, 0.2, 0.01).unwrap();
        assert_eq!(look.adjustment_applied, 1.2);
        assert_eq!(barrier.adjustment_applied, 0.8);
        assert!((look.calculated_fair_value - base.calculated_fair_value * 1.2).abs() < 1e-9);
        assert!((barrier.calculated_fair_value - base.calculated_fair_value * 0.8).abs() < 1e-9);
    }

    #[test]
    fn instrument_inputs_are_guarded() {
        for (spot, strike, term, vol) in [
            (0.0, 90.0, 1.0, 0.2),
            (100.0, 0.0, 1.0, 0.2),
            (100.0, 90.0, 0.0, 0.2),
            (100.0, 90.0, 1.0, 0.0),
        ] {
            assert!(calculate_instrument_fair_value(
                "plain_vanilla",
                spot,
                strike,
                term,
                0.05,
                vol,
                0.0
            )
            .is_err());
        }
    }

    #[test]
    fn performance_conditions() {
        let eps = assess_performance_condition("EPS_growth", 100.0, 60.0, 1.0, 0.9);
        // 0.6 * (1 + (1 - 1/3) * 0.1) = 0.64
        assert!((eps.expected_achievement_probability - 0.64).abs() < 1e-12);
        assert_eq!(eps.expense_multiplier, eps.expected_achievement_probability);

        let tsr = assess_performance_condition("TSR", 100.0, 10.0, 2.0, 0.42);
        assert_eq!(tsr.expected_achievement_probability, 0.42);

        let other = assess_performance_condition("revenue", 100.0, 250.0, 2.0, 0.0);
        assert_eq!(other.expected_achievement_probability, 1.0);
        assert_eq!(other.achievement_rate, 2.5);

        // a period beyond three years gives a negative time share
        let long = assess_performance_condition("EPS_growth", 100.0, 100.0, 6.0, 0.0);
        assert!(long.expected_achievement_probability < 1.0);

        let no_target = assess_performance_condition("revenue", 0.0, 50.0, 1.0, 0.0);
        assert_eq!(no_target.achievement_rate, 0.0);
    }

    #[test]
    fn modification_never_reduces_expense() {
        let m = account_for_grant_modification(
            "2024-01-01",
            5.0,
            10_000,
            "2025-06-30",
            3.0,
            10_000,
            4,
            2,
            20_000.0,
        );
        assert_eq!(m.incremental_fair_value, 0.0);
        assert_eq!(m.prospective_expense, 0.0);
        // per-period 5.0/4 = 1.25, two years = 2.5, already 20,000 expensed
        assert_eq!(m.catch_up_expense, 0.0);
        assert_eq!(m.total_modification_impact, 0.0);
    }

    #[test]
    fn modification_ignores_the_new_option_count() {
        let a = account_for_grant_modification(
            "2024-01-01",
            5.0,
            10_000,
            "2025-06-30",
            8.0,
            10_000,
            4,
            1,
            5_000.0,
        );
        let b = account_for_grant_modification(
            "2024-01-01",
            5.0,
            10_000,
            "2025-06-30",
            8.0,
            50_000,
            4,
            1,
            5_000.0,
        );
        assert_eq!(a.total_modification_impact, b.total_modification_impact);
        assert_eq!(b.new_options, 50_000);
        // incremental 3.0 over the full 4-year term, and over 3 remaining years
        assert_eq!(a.prospective_expense, 1.0);
        assert_eq!(a.remaining_vesting_period, 3);
    }

    #[test]
    fn modification_remaining_period_floors_at_one() {
        let m = account_for_grant_modification(
            "2024-01-01",
            5.0,
            10_000,
            "2026-01-01",
            6.0,
            10_000,
            2,
            5,
            1.0,
        );
        assert_eq!(m.remaining_vesting_period, 1);
        // combined 6.0 / 2 = 3.0 per period, five years elapsed = 15.0
        assert_eq!(m.catch_up_expense, 14.0);

        // a grant already expensed beyond the recalculated figure is not credited back
        let ahead = account_for_grant_modification(
            "2024-01-01",
            5.0,
            10_000,
            "2026-01-01",
            6.0,
            10_000,
            2,
            5,
            1_000.0,
        );
        assert_eq!(ahead.catch_up_expense, 0.0);
    }

    #[test]
    fn group_transactions() {
        let equity = account_for_group_transaction(100_000.0, 0.0, 50_000.0, "equity_for_equity");
        assert_eq!(equity.nci_impact, 0.0);
        assert_eq!(equity.financial_statement_impact, "equity reserve");

        let cash = account_for_group_transaction(0.0, 100_000.0, 50_000.0, "cash_for_equity");
        assert_eq!(cash.nci_impact, 5_000.0);
        assert_eq!(cash.financial_statement_impact, "liability");
        assert_eq!(cash.treatment, "cash-settled - remeasurement required");
    }
}
