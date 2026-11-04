//! amortization-service `/calculate`: straight line and reducing balance.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vimbai_common::Money;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmortizationAsset {
    pub intangible_id: String,
    #[serde(default)]
    pub r#type: String,
    pub cost: Money,
    #[serde(default)]
    pub residual_value: Money,
    #[serde(default)]
    pub useful_life_years: i64,
    #[serde(default)]
    pub accumulated_amortization: Money,
    /// reducing-balance rate, exact so a 25% charge is a quarter and not the
    /// nearest double to 0.25
    #[serde(default)]
    pub rate: Money,
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
///
/// The charges are decimals: a schedule adds one year's charge to the next, and
/// the totals here sum every asset on the register, which is the kind of
/// accumulation that drifts in a double once the amounts have many digits.
pub fn amortization(intangibles: &[AmortizationAsset]) -> AmortizationOutput {
    let zero = Money::zero();
    let mut annuals: Vec<Money> = Vec::with_capacity(intangibles.len());
    let mut nbvs: Vec<Money> = Vec::with_capacity(intangibles.len());
    let mut rows = Vec::with_capacity(intangibles.len());
    for a in intangibles {
        let depreciable = a.cost.minus(&a.residual_value);
        let annual = if a.method == "straight_line" {
            if a.useful_life_years > 0 {
                depreciable.over(&Money::from_i64(a.useful_life_years))
            } else {
                zero.clone()
            }
        } else {
            a.cost.minus(&a.accumulated_amortization).times(&a.rate)
        };
        let new_accumulated = a.accumulated_amortization.plus(&annual);
        let nbv = a.cost.minus(&new_accumulated);
        let remaining_years = if annual > zero {
            nbv.over(&annual).round(1).to_f64_lossy()
        } else {
            0.0
        };
        annuals.push(annual.clone());
        nbvs.push(nbv.clone());
        rows.push(AmortizationRow {
            intangible_id: a.intangible_id.clone(),
            r#type: a.r#type.clone(),
            cost: a.cost.to_f64_lossy(),
            residual_value: a.residual_value.to_f64_lossy(),
            annual_amortization: annual.round(2).to_f64_lossy(),
            monthly_amortization: annual.over(&Money::from_i64(12)).round(2).to_f64_lossy(),
            accumulated_amortization: new_accumulated.round(2).to_f64_lossy(),
            net_book_value: nbv.round(2).to_f64_lossy(),
            method: a.method.clone(),
            remaining_life_years: remaining_years,
        });
    }
    AmortizationOutput {
        total_amortization: Money::sum(&annuals).round(2).to_f64_lossy(),
        total_net_book_value: Money::sum(&nbvs).round(2).to_f64_lossy(),
        schedules: rows,
    }
}

/// The service's `/calculate` body, in and out, so the Rust API can answer the
/// same request the Python service did. The period is only echoed back.
pub fn amortization_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Request {
        company_id: String,
        intangibles: Vec<AmortizationAsset>,
        period_start: String,
        period_end: String,
    }
    let request: Request =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    let output = amortization(&request.intangibles);
    Ok(serde_json::json!({
        "company_id": request.company_id,
        "period": {"start": request.period_start, "end": request.period_end},
        "total_amortization": output.total_amortization,
        "total_net_book_value": output.total_net_book_value,
        "schedules": output.schedules,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(method: &str) -> AmortizationAsset {
        AmortizationAsset {
            intangible_id: "i1".into(),
            r#type: "software".into(),
            cost: Money::from_i64(48_000),
            residual_value: Money::zero(),
            useful_life_years: 4,
            accumulated_amortization: Money::from_i64(10_000),
            rate: Money::parse("0.25").unwrap(),
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
