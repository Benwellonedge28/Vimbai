//! share-options-service `/analyze`: IFRS 2 style grant costing with
//! equity/cash settlement split and dilution flag.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionGrant {
    pub id: String,
    pub number_of_options: f64,
    pub fair_value: f64,
    #[serde(default = "one")]
    pub vesting_period: f64,
    #[serde(default = "one")]
    pub vesting_probability: f64,
    #[serde(default)]
    pub exercise_price: f64,
    #[serde(default)]
    pub cash_settled: bool,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareOptionsRequest {
    pub company_id: String,
    pub period: String,
    pub share_price: f64,
    #[serde(default)]
    pub option_grants: Vec<OptionGrant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantLine {
    pub grant_id: String,
    pub number_of_options: f64,
    pub fair_value_per_option: f64,
    pub vesting_period: f64,
    pub exercise_price: f64,
    pub total_compensation_cost: f64,
    pub vesting_probability: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareOptionsResult {
    pub company_id: String,
    pub period: String,
    pub option_grants: Vec<GrantLine>,
    pub total_compensation_cost: f64,
    pub equity_settled_expense: f64,
    pub cash_settled_expense: f64,
    pub dilutive_effect: f64,
    pub recommendations: Vec<String>,
}

pub fn analyze_share_options(req: &ShareOptionsRequest) -> ShareOptionsResult {
    let mut lines = Vec::new();
    let mut total_cost = 0.0;
    let mut equity_expense = 0.0;
    let mut cash_expense = 0.0;
    let mut dilutive = 0.0;

    for g in &req.option_grants {
        let vesting_period = if g.vesting_period != 0.0 {
            g.vesting_period
        } else {
            1.0
        };
        let total_grant_cost = g.number_of_options * g.fair_value;
        // Python divides by vesting_period unguarded: a 0 there raises.
        let vesting_factor = total_grant_cost / vesting_period * g.vesting_probability;
        total_cost += total_grant_cost;
        if g.cash_settled {
            cash_expense += vesting_factor;
        } else {
            equity_expense += vesting_factor;
            if req.share_price > g.exercise_price {
                dilutive += g.number_of_options;
            }
        }
        lines.push(GrantLine {
            grant_id: g.id.clone(),
            number_of_options: g.number_of_options,
            fair_value_per_option: g.fair_value,
            vesting_period: g.vesting_period,
            exercise_price: g.exercise_price,
            total_compensation_cost: py_round2(total_grant_cost),
            vesting_probability: g.vesting_probability,
        });
    }

    ShareOptionsResult {
        company_id: req.company_id.clone(),
        period: req.period.clone(),
        option_grants: lines,
        total_compensation_cost: py_round2(total_cost),
        equity_settled_expense: py_round2(equity_expense),
        cash_settled_expense: py_round2(cash_expense),
        dilutive_effect: dilutive,
        recommendations: vec![
            "Ensure fair value calculations are documented".into(),
            "Review vesting conditions".into(),
            "Update option pricing model assumptions".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equity_and_cash_split() {
        let r = analyze_share_options(&ShareOptionsRequest {
            company_id: "c1".into(),
            period: "2026".into(),
            share_price: 12.0,
            option_grants: vec![
                OptionGrant {
                    id: "g1".into(),
                    number_of_options: 1000.0,
                    fair_value: 3.0,
                    vesting_period: 3.0,
                    vesting_probability: 0.9,
                    exercise_price: 10.0,
                    cash_settled: false,
                },
                OptionGrant {
                    id: "g2".into(),
                    number_of_options: 500.0,
                    fair_value: 2.0,
                    vesting_period: 2.0,
                    vesting_probability: 1.0,
                    exercise_price: 8.0,
                    cash_settled: true,
                },
            ],
        });
        // g1: cost 3000, factor 3000/3*0.9 = 900, dilutive (12 > 10)
        assert_eq!(r.total_compensation_cost, 4000.0);
        assert_eq!(r.equity_settled_expense, 900.0);
        assert_eq!(r.cash_settled_expense, 500.0);
        assert_eq!(r.dilutive_effect, 1000.0);
    }

    #[test]
    fn out_of_the_money_grant_not_dilutive() {
        let r = analyze_share_options(&ShareOptionsRequest {
            company_id: "c1".into(),
            period: "2026".into(),
            share_price: 5.0,
            option_grants: vec![OptionGrant {
                id: "g1".into(),
                number_of_options: 100.0,
                fair_value: 1.0,
                vesting_period: 1.0,
                vesting_probability: 1.0,
                exercise_price: 20.0,
                cash_settled: false,
            }],
        });
        assert_eq!(r.dilutive_effect, 0.0);
        assert_eq!(r.equity_settled_expense, 100.0);
    }

    #[test]
    fn vesting_probability_scales_expense() {
        let r = analyze_share_options(&ShareOptionsRequest {
            company_id: "c1".into(),
            period: "2026".into(),
            share_price: 1.0,
            option_grants: vec![OptionGrant {
                id: "g1".into(),
                number_of_options: 1000.0,
                fair_value: 2.0,
                vesting_period: 4.0,
                vesting_probability: 0.5,
                exercise_price: 5.0,
                cash_settled: false,
            }],
        });
        // 2000/4*0.5 = 250
        assert_eq!(r.equity_settled_expense, 250.0);
    }
}
