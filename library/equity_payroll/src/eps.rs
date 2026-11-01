//! eps-service `/calculate`: basic, continuing and diluted EPS with
//! the treasury-stock method.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OptionInput {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub exercise_price: f64,
    #[serde(default)]
    pub average_market_price: f64,
    #[serde(default)]
    pub number_of_options: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AntiDilutiveItem {
    #[serde(default)]
    pub anti_dilutive_shares: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpsOutput {
    pub basic_eps: f64,
    pub diluted_eps: f64,
    pub eps_from_continuing_operations: f64,
    pub weighted_average_shares: f64,
    pub diluted_shares: f64,
    pub dilutive_effect: f64,
    pub anti_dilutive_effect: f64,
    pub potential_dilutive_instruments: Vec<OptionInput>,
}

pub fn calculate_eps(
    net_profit_attributable: f64,
    discontinued_operations_profit: f64,
    weighted_average_shares: f64,
    potential: &[OptionInput],
    anti_dilutive: &[AntiDilutiveItem],
) -> EpsOutput {
    let basic = if weighted_average_shares != 0.0 {
        net_profit_attributable / weighted_average_shares
    } else {
        0.0
    };
    let continuing = net_profit_attributable - discontinued_operations_profit;
    let eps_cont = if weighted_average_shares != 0.0 {
        continuing / weighted_average_shares
    } else {
        0.0
    };

    let mut dilutive_effect: f64 = 0.0;
    let mut diluted_shares = weighted_average_shares;
    let mut instruments = Vec::new();

    for opt in potential {
        if opt.average_market_price > opt.exercise_price {
            // Python: int() truncates toward zero.
            let treasury = (opt.number_of_options
                * (1.0 - opt.exercise_price / opt.average_market_price))
                as i64;
            dilutive_effect += treasury as f64;
            instruments.push(OptionInput {
                id: opt.id.clone(),
                exercise_price: opt.exercise_price,
                average_market_price: opt.average_market_price,
                number_of_options: opt.number_of_options,
            });
        }
    }

    for item in anti_dilutive {
        dilutive_effect -= item.anti_dilutive_shares;
    }

    diluted_shares += dilutive_effect.max(0.0);
    let diluted = if diluted_shares != 0.0 {
        net_profit_attributable / diluted_shares
    } else {
        0.0
    };
    let anti_total: f64 = anti_dilutive.iter().map(|a| a.anti_dilutive_shares).sum();

    EpsOutput {
        basic_eps: py_round(basic, 4),
        diluted_eps: py_round(diluted, 4),
        eps_from_continuing_operations: py_round(eps_cont, 4),
        weighted_average_shares,
        diluted_shares,
        dilutive_effect: dilutive_effect.max(0.0),
        anti_dilutive_effect: anti_total.abs().max(0.0),
        potential_dilutive_instruments: instruments,
    }
}

impl EpsOutput {
    #[cfg(test)]
    fn instruments_len(&self) -> usize {
        self.potential_dilutive_instruments.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(price: f64, market: f64, count: f64) -> OptionInput {
        OptionInput {
            id: "o".into(),
            exercise_price: price,
            average_market_price: market,
            number_of_options: count,
        }
    }

    #[test]
    fn basic_and_continuing() {
        let r = calculate_eps(1_000_000.0, 200_000.0, 500_000.0, &[], &[]);
        assert_eq!(r.basic_eps, 2.0);
        assert_eq!(r.eps_from_continuing_operations, 1.6);
        assert_eq!(r.diluted_shares, 500_000.0);
        assert_eq!(r.diluted_eps, 2.0);
    }

    #[test]
    fn treasury_stock_method_truncates() {
        // 1000 * (1 - 50/60) = 166.666 -> 166
        let r = calculate_eps(
            1_000_000.0,
            0.0,
            100_000.0,
            &[opt(50.0, 60.0, 1_000.0)],
            &[],
        );
        assert_eq!(r.dilutive_effect, 166.0);
        assert_eq!(r.diluted_shares, 100_166.0);
        assert_eq!(r.instruments_len(), 1);
        assert!(r.diluted_eps < r.basic_eps);
    }

    #[test]
    fn out_of_the_money_options_ignored() {
        let r = calculate_eps(100.0, 0.0, 100.0, &[opt(80.0, 60.0, 1_000.0)], &[]);
        assert_eq!(r.dilutive_effect, 0.0);
        assert!(r.potential_dilutive_instruments.is_empty());
    }

    #[test]
    fn anti_dilutive_floors_at_zero_effect() {
        let r = calculate_eps(
            100_000.0,
            0.0,
            10_000.0,
            &[],
            &[AntiDilutiveItem {
                anti_dilutive_shares: 500.0,
            }],
        );
        assert_eq!(r.dilutive_effect, 0.0);
        assert_eq!(r.diluted_shares, 10_000.0);
        assert_eq!(r.anti_dilutive_effect, 500.0);
    }

    #[test]
    fn zero_shares_guard() {
        let r = calculate_eps(5_000.0, 0.0, 0.0, &[], &[]);
        assert_eq!(r.basic_eps, 0.0);
        assert_eq!(r.diluted_eps, 0.0);
    }
}
