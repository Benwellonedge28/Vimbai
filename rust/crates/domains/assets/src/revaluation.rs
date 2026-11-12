//! revaluation-model-service `/revalue`: IAS 16 revaluation with
//! cumulative surplus absorbing subsequent losses.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevaluationAsset {
    pub asset_id: String,
    #[serde(default)]
    pub asset_name: String,
    pub original_cost: f64,
    #[serde(default)]
    pub accumulated_depreciation: f64,
    #[serde(default)]
    pub residual_value: f64,
    #[serde(default = "default_life")]
    pub useful_life_years: i64,
}
fn default_life() -> i64 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetBook {
    pub asset_id: String,
    pub fair_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevaluationRow {
    pub asset_id: String,
    pub asset_name: String,
    pub previous_carrying_amount: f64,
    pub fair_value: f64,
    pub revaluation_gain: f64,
    /// absorbed by prior surplus vs charged to P&L
    pub absorbed_by_surplus: f64,
    pub charged_to_pnl: f64,
    pub cumulative_surplus: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevaluationOutput {
    pub total_gain: f64,
    pub total_loss: f64,
    pub cumulative_surplus: f64,
    pub rows: Vec<RevaluationRow>,
}

/// Gains add to cumulative surplus; losses first absorb prior surplus,
/// remainder charged to P&L. Missing fair values fall back to cost.
pub fn revalue(assets: &[RevaluationAsset], fair_values: &[AssetBook]) -> RevaluationOutput {
    let mut total_gain = 0.0;
    let mut total_loss = 0.0;
    let mut surplus = 0.0;
    let mut rows = Vec::with_capacity(assets.len());

    for a in assets {
        let fair = fair_values
            .iter()
            .find(|f| f.asset_id == a.asset_id)
            .map(|f| f.fair_value)
            .unwrap_or(a.original_cost);
        let carrying = a.original_cost - a.accumulated_depreciation;
        let (gain, absorbed, charged) = if fair > carrying {
            let g = fair - carrying;
            total_gain += g;
            surplus += g;
            (g, 0.0, 0.0)
        } else if fair < carrying {
            let loss = carrying - fair;
            total_loss += loss;
            let absorbed = loss.min(surplus);
            let charged = loss - absorbed;
            surplus -= absorbed;
            (0.0, absorbed, charged)
        } else {
            (0.0, 0.0, 0.0)
        };
        rows.push(RevaluationRow {
            asset_id: a.asset_id.clone(),
            asset_name: a.asset_name.clone(),
            previous_carrying_amount: py_round2(carrying),
            fair_value: py_round2(fair),
            revaluation_gain: py_round2(gain),
            absorbed_by_surplus: py_round2(absorbed),
            charged_to_pnl: py_round2(charged),
            cumulative_surplus: py_round2(surplus),
        });
    }

    RevaluationOutput {
        total_gain: py_round2(total_gain),
        total_loss: py_round2(total_loss),
        cumulative_surplus: py_round2(surplus),
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(id: &str, cost: f64, acc: f64) -> RevaluationAsset {
        RevaluationAsset {
            asset_id: id.into(),
            asset_name: id.into(),
            original_cost: cost,
            accumulated_depreciation: acc,
            residual_value: 0.0,
            useful_life_years: 5,
        }
    }

    #[test]
    fn gain_to_surplus() {
        let r = revalue(
            &[asset("a", 100000.0, 20000.0)],
            &[AssetBook {
                asset_id: "a".into(),
                fair_value: 100000.0,
            }],
        );
        assert_eq!(r.total_gain, 20000.0);
        assert_eq!(r.cumulative_surplus, 20000.0);
        assert_eq!(r.rows[0].cumulative_surplus, 20000.0);
    }

    #[test]
    fn loss_absorbs_prior_surplus_first() {
        let r = revalue(
            &[asset("a", 100000.0, 20000.0), asset("b", 50000.0, 0.0)],
            &[
                AssetBook {
                    asset_id: "a".into(),
                    fair_value: 100000.0,
                },
                AssetBook {
                    asset_id: "b".into(),
                    fair_value: 30000.0,
                },
            ],
        );
        // gain 20k; loss 20k absorbed -> 0 charged
        assert_eq!(r.rows[1].absorbed_by_surplus, 20000.0);
        assert_eq!(r.rows[1].charged_to_pnl, 0.0);
        assert_eq!(r.cumulative_surplus, 0.0);
    }

    #[test]
    fn missing_fair_value_uses_cost() {
        let r = revalue(&[asset("a", 100000.0, 30000.0)], &[]);
        // fair = cost 100000 > carrying 70000 -> gain 30000
        assert_eq!(r.total_gain, 30000.0);
    }
}
