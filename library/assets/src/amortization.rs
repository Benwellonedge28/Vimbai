//! amortization-service `/calculate`: straight line and reducing balance.
use serde::{Deserialize, Serialize};
use vimbai_common::{py_round, py_round2};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmortizationAsset {
    pub intangible_id: String,
    #[serde(default)]
    pub r#type: String,
    pub cost: f64,
    #[serde(default)]
    pub residual_value: f64,
    #[serde(default)]
    pub useful_life_years: i64,
    #[serde(default)]
    pub accumulated_amortization: f64,
    /// reducing-balance rate
    #[serde(default)]
    pub rate: f64,
    #[serde(default)]
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmortizationRow {
    pub intangible_id: String,
    pub r#type: String,
    pub cost: f64,
    pub residual_value: f64,
    pub annual_amortization: f64,
    pub monthly_amortization: f64,
    pub accumulated_amortization: f64,
    pub net_book_value: f64,
    pub method: String,
    pub remaining_life_years: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmortizationOutput {
    pub total_amortization: f64,
    pub total_net_book_value: f64,
    pub schedules: Vec<AmortizationRow>,
}

/// Straight line annual = (cost - residual) / life (0 if life <= 0);
/// reducing balance annual = (cost - accumulated) * rate. NBV can go
/// negative when accumulated exceeds cost; Python does not floor it.
pub fn amortization(intangibles: &[AmortizationAsset]) -> AmortizationOutput {
    let mut total = 0.0;
    let mut total_nbv = 0.0;
    let mut rows = Vec::with_capacity(intangibles.len());
    for a in intangibles {
        let depreciable = a.cost - a.residual_value;
        let annual = if a.method == "straight_line" {
            if a.useful_life_years > 0 {
                depreciable / a.useful_life_years as f64
            } else {
                0.0
            }
        } else {
            (a.cost - a.accumulated_amortization) * a.rate
        };
        let new_accumulated = a.accumulated_amortization + annual;
        let nbv = a.cost - new_accumulated;
        let remaining_years = if annual > 0.0 { nbv / annual } else { 0.0 };
        total += annual;
        total_nbv += nbv;
        rows.push(AmortizationRow {
            intangible_id: a.intangible_id.clone(),
            r#type: a.r#type.clone(),
            cost: a.cost,
            residual_value: a.residual_value,
            annual_amortization: py_round2(annual),
            monthly_amortization: py_round2(annual / 12.0),
            accumulated_amortization: py_round2(new_accumulated),
            net_book_value: py_round2(nbv),
            method: a.method.clone(),
            remaining_life_years: py_round(remaining_years, 1),
        });
    }
    AmortizationOutput {
        total_amortization: py_round2(total),
        total_net_book_value: py_round2(total_nbv),
        schedules: rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(method: &str) -> AmortizationAsset {
        AmortizationAsset {
            intangible_id: "i1".into(),
            r#type: "software".into(),
            cost: 48000.0,
            residual_value: 0.0,
            useful_life_years: 4,
            accumulated_amortization: 10000.0,
            rate: 0.25,
            method: method.into(),
        }
    }

    #[test]
    fn straight_line() {
        let r = amortization(&[a("straight_line")]);
        assert_eq!(r.total_amortization, 12000.0);
        assert_eq!(r.schedules[0].monthly_amortization, 1000.0);
        assert_eq!(r.schedules[0].accumulated_amortization, 22000.0);
        assert_eq!(r.schedules[0].net_book_value, 26000.0);
    }

    #[test]
    fn reducing_balance_uses_carrying_amount() {
        let r = amortization(&[a("reducing_balance")]);
        // (48000-10000)*0.25 = 9500
        assert_eq!(r.schedules[0].annual_amortization, 9500.0);
        assert_eq!(r.schedules[0].net_book_value, 28500.0);
    }

    #[test]
    fn zero_life_guarded() {
        let mut x = a("straight_line");
        x.useful_life_years = 0;
        let r = amortization(&[x]);
        assert_eq!(r.total_amortization, 0.0);
    }
}
