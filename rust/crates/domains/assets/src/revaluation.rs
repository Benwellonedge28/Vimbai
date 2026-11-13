//! revaluation-model-service: IAS 16 revaluations, and the four questions that follow one.
//!
//! One pass over a company's assets decides, for each, whether its fair value sits above or below the carrying amount
//! it has reached. Gains go to a revaluation surplus in other comprehensive income; losses go first against that
//! surplus and only the rest to profit or loss. The same pass reports the depreciation the revalued amount will now
//! carry. The companions then answer what follows: the surplus after deferred tax, the depreciation over the remaining
//! life, what an impairment does when a surplus is standing, and the indexation allowance on a disposal.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * an asset with no fair value given is measured at its original cost, which for a depreciated asset reports a gain;
//! * the carrying amount is the original cost less the accumulated depreciation the caller supplies - the service never
//!   worked out depreciation itself;
//! * the surplus is a running figure in the order the assets are listed, so a loss only finds a surplus an earlier gain
//!   in the same list created;
//! * a loss's treatment is "mixed" when any of it was absorbed and "to_profit_or_loss" when none was;
//! * depreciation is adjusted only for assets revalued *downward*, which is what the service did - an upward
//!   revaluation reports no adjustment even though a real one would carry more depreciation;
//! * the remaining life counts from a fixed two years held, not from the dates, so a six-year asset has four years left;
//! * a rate over nothing reports nil rather than dividing, where the service would have failed the request;
//! * the two flags a caller can send - whether to check the revaluation frequency, and the regularity - carry no
//!   arithmetic; they were accepted and unused, and are accepted here too.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;

/// The two years the service assumed every asset had been held for when it worked out what life was left.
const YEARS_HELD: i64 = 2;

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

/// What one asset's revaluation came to, with the treatment it earns.
#[derive(Debug, Clone, PartialEq)]
pub struct RevaluationRow {
    pub asset_id: String,
    pub asset_name: String,
    pub previous_carrying_amount: Money,
    pub fair_value: Money,
    /// nil for a loss
    pub revaluation_gain: Money,
    /// nil for a gain
    pub revaluation_loss: Money,
    pub treatment: &'static str,
    pub absorbed_by_surplus: Money,
    pub charged_to_pnl: Money,
    /// the running surplus after this asset
    pub cumulative_surplus: Money,
    /// the depreciation the revalued amount now carries, for a downward revaluation
    pub depreciation_adjustment: Option<DepreciationAdjustment>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DepreciationAdjustment {
    pub remaining_useful_life: i64,
    pub current_depreciation_rate: Money,
    pub new_depreciation_rate: Money,
    pub new_annual_depreciation: Money,
    pub accumulated_depreciation_adjustment: Money,
}

/// The whole answer for a revaluation: what each asset did, and the totals across them.
#[derive(Debug, Clone)]
pub struct Revaluation {
    pub rows: Vec<RevaluationRow>,
    pub total_gain: Money,
    pub total_loss: Money,
    pub surplus: Money,
    /// the part of the gains that stays in other comprehensive income
    pub surplus_oci: Money,
    /// the part of the losses that went to profit or loss
    pub profit_or_loss_impact: Money,
}

fn money(value: f64) -> Money {
    Money::from_f64_shortest(value).unwrap_or_else(|_| Money::zero())
}

/// A rate as a percentage of a base, nil when there is no base rather than a division.
fn rate_of(part: &Money, base: &Money) -> Money {
    if base.is_zero() {
        return Money::zero();
    }
    part.over(base).times(&money(100.0))
}

/// One pass over the assets: gains to the surplus, losses against it first, and the depreciation each revalued
/// amount now carries.
pub fn revaluation(assets: &[RevaluationAsset], fair_values: &[AssetBook]) -> Revaluation {
    let mut total_gain = Money::zero();
    let mut total_loss = Money::zero();
    let mut surplus = Money::zero();
    let mut absorbed_in_total = Money::zero();
    let mut charged_in_total = Money::zero();
    let mut rows = Vec::with_capacity(assets.len());

    for asset in assets {
        let fair = fair_values
            .iter()
            .find(|book| book.asset_id == asset.asset_id)
            .map(|book| money(book.fair_value))
            .unwrap_or_else(|| money(asset.original_cost));
        let carrying = money(asset.original_cost).minus(&money(asset.accumulated_depreciation));

        let (gain, loss, absorbed, charged, treatment) = if fair.gt(&carrying) {
            let gain = fair.minus(&carrying);
            total_gain = total_gain.plus(&gain);
            surplus = surplus.plus(&gain);
            (
                gain,
                Money::zero(),
                Money::zero(),
                Money::zero(),
                "to_revaluation_surplus",
            )
        } else if fair.lt(&carrying) {
            let loss = carrying.minus(&fair);
            total_loss = total_loss.plus(&loss);
            let absorbed = if loss.gt(&surplus) {
                surplus.clone()
            } else {
                loss.clone()
            };
            let charged = loss.minus(&absorbed);
            surplus = surplus.minus(&absorbed);
            absorbed_in_total = absorbed_in_total.plus(&absorbed);
            charged_in_total = charged_in_total.plus(&charged);
            let treatment = if absorbed.gt(&Money::zero()) {
                "mixed"
            } else {
                "to_profit_or_loss"
            };
            (Money::zero(), loss, absorbed, charged, treatment)
        } else {
            (
                Money::zero(),
                Money::zero(),
                Money::zero(),
                Money::zero(),
                "none",
            )
        };

        // the service adjusted depreciation only where the asset was written down
        let adjustment = if treatment != "to_revaluation_surplus" && treatment != "none" {
            let remaining = std::cmp::max(1, asset.useful_life_years - YEARS_HELD);
            let annual = money(asset.original_cost)
                .minus(&money(asset.residual_value))
                .over(&money(asset.useful_life_years.max(1) as f64));
            let new_annual = fair
                .minus(&money(asset.residual_value))
                .over(&money(remaining as f64));
            Some(DepreciationAdjustment {
                remaining_useful_life: remaining,
                current_depreciation_rate: rate_of(&annual, &carrying),
                new_depreciation_rate: rate_of(&new_annual, &fair),
                new_annual_depreciation: new_annual,
                accumulated_depreciation_adjustment: Money::zero(),
            })
        } else {
            None
        };

        rows.push(RevaluationRow {
            asset_id: asset.asset_id.clone(),
            asset_name: asset.asset_name.clone(),
            previous_carrying_amount: carrying,
            fair_value: fair,
            revaluation_gain: gain,
            revaluation_loss: loss,
            treatment,
            absorbed_by_surplus: absorbed,
            charged_to_pnl: charged,
            cumulative_surplus: surplus.clone(),
            depreciation_adjustment: adjustment,
        });
    }

    Revaluation {
        rows,
        total_gain: total_gain.clone(),
        total_loss,
        surplus,
        surplus_oci: total_gain.minus(&absorbed_in_total),
        profit_or_loss_impact: charged_in_total,
    }
}

/// `POST /revalue`.
pub fn revalue_from_json(payload: &Value) -> Result<Value, String> {
    let assets: Vec<RevaluationAsset> = serde_json::from_value(
        payload
            .get("assets")
            .cloned()
            .ok_or_else(|| "assets is required".to_string())?,
    )
    .map_err(|e| format!("invalid asset: {e}"))?;
    let fair_values: Vec<AssetBook> = serde_json::from_value(
        payload
            .get("fair_values")
            .cloned()
            .unwrap_or_else(|| json!({})),
    )
    .or_else(|_| {
        // the service took the fair values as a map keyed by asset, so accept that shape as well
        let map = payload
            .get("fair_values")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let books: Vec<AssetBook> = map
            .as_object()
            .map(|fields| {
                fields
                    .iter()
                    .filter_map(|(asset_id, value)| {
                        value.as_f64().map(|fair_value| AssetBook {
                            asset_id: asset_id.clone(),
                            fair_value,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok::<_, serde_json::Error>(books)
    })
    .map_err(|e| format!("invalid fair value: {e}"))?;

    let answer = revaluation(&assets, &fair_values);

    let mut gains = Vec::new();
    let mut losses = Vec::new();
    let mut adjustments = Vec::new();
    let mut carrying_amounts = serde_json::Map::new();
    for row in &answer.rows {
        carrying_amounts.insert(row.asset_id.clone(), wire(&row.fair_value));
        if !row.revaluation_gain.is_zero() {
            gains.push(json!({
                "asset_id": row.asset_id,
                "asset_name": row.asset_name,
                "previous_carrying_amount": wire(&row.previous_carrying_amount),
                "fair_value": wire(&row.fair_value),
                "revaluation_gain": wire(&row.revaluation_gain),
                "treatment": row.treatment,
                "cumulative_surplus": wire(&row.cumulative_surplus),
            }));
        }
        if !row.revaluation_loss.is_zero() {
            losses.push(json!({
                "asset_id": row.asset_id,
                "asset_name": row.asset_name,
                "previous_carrying_amount": wire(&row.previous_carrying_amount),
                "fair_value": wire(&row.fair_value),
                "revaluation_loss": wire(&row.revaluation_loss),
                "treatment": row.treatment,
                "absorbed_from_surplus": wire(&row.absorbed_by_surplus),
                "charged_to_profit": wire(&row.charged_to_pnl),
            }));
        }
        if let Some(adjustment) = &row.depreciation_adjustment {
            adjustments.push(json!({
                "asset_id": row.asset_id,
                "new_useful_life": assets
                    .iter()
                    .find(|asset| asset.asset_id == row.asset_id)
                    .map(|asset| asset.useful_life_years)
                    .unwrap_or(0),
                "remaining_useful_life": adjustment.remaining_useful_life,
                "current_depreciation_rate": wire(&adjustment.current_depreciation_rate),
                "new_depreciation_rate": wire(&adjustment.new_depreciation_rate),
                "new_annual_depreciation": wire(&adjustment.new_annual_depreciation),
                "accumulated_depreciation_adjustment": wire(&adjustment.accumulated_depreciation_adjustment),
            }));
        }
    }

    Ok(json!({
        "company_id": payload.get("company_id").cloned().unwrap_or(Value::Null),
        "revaluation_date": payload.get("revaluation_date").cloned().unwrap_or(Value::Null),
        "gains": gains,
        "losses": losses,
        "depreciation_adjustments": adjustments,
        "total_revaluation_gain": wire(&answer.total_gain),
        "total_revaluation_loss": wire(&answer.total_loss),
        "net_revaluation_effect": wire(&answer.total_gain.minus(&answer.total_loss)),
        "revaluation_surplus_OCI": wire(&answer.surplus_oci),
        "profit_or_loss_impact": wire(&answer.profit_or_loss_impact),
        "new_carrying_amounts": Value::Object(carrying_amounts),
    }))
}

fn figure(payload: &Value, key: &str) -> Money {
    match payload.get(key) {
        Some(Value::Number(number)) => number.as_f64().map(money).unwrap_or_else(Money::zero),
        Some(Value::String(text)) => Money::parse(text).unwrap_or_else(|_| Money::zero()),
        _ => Money::zero(),
    }
}

/// `POST /revaluation-surplus`: the surplus left after the deferred tax on it.
pub fn surplus_from_json(payload: &Value) -> Result<Value, String> {
    let gain = figure(payload, "revaluation_gain");
    let tax_rate = figure(payload, "deferred_tax_rate");
    let previous = figure(payload, "previous_surplus");
    let absorbed = figure(payload, "losses_absorbed");

    let net = gain.minus(&absorbed);
    let deferred_tax = net.times(&tax_rate);
    let surplus = net.minus(&deferred_tax);

    Ok(json!({
        "gross_revaluation_gain": wire(&gain),
        "losses_absorbed_from_surplus": wire(&absorbed),
        "net_gain": wire(&net),
        "deferred_tax_liability": wire(&deferred_tax),
        "revaluation_surplus_OCI": wire(&surplus),
        "previous_cumulative_surplus": wire(&previous),
        "total_revaluation_surplus": wire(&previous.plus(&surplus)),
    }))
}

/// `POST /depreciation-after-revaluation`: the charge the revalued amount will now carry.
pub fn depreciation_after_from_json(payload: &Value) -> Result<Value, String> {
    let revalued = figure(payload, "revalued_amount");
    let residual = figure(payload, "residual_value");
    let remaining = payload
        .get("remaining_useful_life")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let total_life = payload
        .get("total_useful_life")
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let depreciable = revalued.minus(&residual);
    // no life left means no charge rather than a division
    let annual = if remaining > 0 {
        depreciable.over(&money(remaining as f64))
    } else {
        Money::zero()
    };

    Ok(json!({
        "revalued_amount": wire(&revalued),
        "residual_value": wire(&residual),
        "depreciable_amount": wire(&depreciable),
        "remaining_useful_life": remaining,
        "annual_depreciation": wire(&annual),
        "depreciation_rate": wire(&rate_of(&annual, &depreciable)),
        "monthly_depreciation": wire(&annual.over(&money(12.0))),
        "total_life_expired": wire(&if total_life > 0 {
            money(((total_life - remaining) as f64 / total_life as f64) * 100.0)
        } else {
            Money::zero()
        }),
    }))
}

/// `POST /impairment-after-revaluation`: an impairment, against any surplus standing first.
pub fn impairment_after_from_json(payload: &Value) -> Result<Value, String> {
    let carrying = figure(payload, "carrying_amount");
    let recoverable = figure(payload, "recoverable_amount");
    let surplus_available = figure(payload, "revaluation_surplus_available");

    let loss = carrying.minus(&recoverable);
    if !loss.gt(&Money::zero()) {
        // nothing to write down: the service answered with the two flags and no figures
        return Ok(json!({
            "carrying_amount": wire(&carrying),
            "recoverable_amount": wire(&recoverable),
            "impairment_loss": wire(&Money::zero()),
            "recoverable_amount_is_higher": true,
            "no_impairment_required": true,
        }));
    }

    let absorbed = if loss.gt(&surplus_available) {
        surplus_available.clone()
    } else {
        loss.clone()
    };
    let charged = loss.minus(&absorbed);

    Ok(json!({
        "carrying_amount": wire(&carrying),
        "recoverable_amount": wire(&recoverable),
        "impairment_loss": wire(&loss),
        "absorbed_from_revaluation_surplus": wire(&absorbed),
        "charged_to_profit_or_loss": wire(&charged),
        "remaining_surplus": wire(&surplus_available.minus(&absorbed)),
    }))
}

/// `POST /indexation-allowance`: the allowance a disposal earns for the rise in prices since acquisition.
pub fn indexation_from_json(payload: &Value) -> Result<Value, String> {
    let cost = figure(payload, "original_cost");
    let at_acquisition = figure(payload, "cpi_at_acquisition");
    let at_disposal = figure(payload, "cpi_at_disposal");

    // no index to divide by leaves the allowance at nil
    let factor = if at_acquisition.gt(&Money::zero()) {
        at_disposal.over(&at_acquisition)
    } else {
        money(1.0)
    };
    let indexed = cost.times(&factor);

    Ok(json!({
        "original_cost": wire(&cost),
        "original_date": payload.get("original_date").cloned().unwrap_or(Value::Null),
        "disposal_date": payload.get("disposal_date").cloned().unwrap_or(Value::Null),
        "cpi_at_acquisition": wire(&at_acquisition),
        "cpi_at_disposal": wire(&at_disposal),
        "indexation_factor": wire(&factor),
        "indexed_cost": wire(&indexed),
        "indexation_allowance": wire(&indexed.minus(&cost)),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(id: &str, cost: f64, accumulated: f64) -> RevaluationAsset {
        RevaluationAsset {
            asset_id: id.into(),
            asset_name: id.into(),
            original_cost: cost,
            accumulated_depreciation: accumulated,
            residual_value: 0.0,
            useful_life_years: 6,
        }
    }

    fn fair(id: &str, value: f64) -> AssetBook {
        AssetBook {
            asset_id: id.into(),
            fair_value: value,
        }
    }

    #[test]
    fn a_gain_goes_to_the_surplus_in_other_comprehensive_income() {
        let answer = revaluation(&[asset("a", 100_000.0, 20_000.0)], &[fair("a", 100_000.0)]);
        assert_eq!(answer.total_gain.to_fixed(2), "20000.00");
        assert_eq!(answer.surplus_oci.to_fixed(2), "20000.00");
        assert_eq!(answer.profit_or_loss_impact.to_fixed(0), "0");
        assert_eq!(answer.rows[0].treatment, "to_revaluation_surplus");
    }

    #[test]
    fn a_loss_uses_the_surplus_an_earlier_gain_in_the_same_list_created() {
        let answer = revaluation(
            &[asset("a", 100_000.0, 20_000.0), asset("b", 50_000.0, 0.0)],
            &[fair("a", 100_000.0), fair("b", 30_000.0)],
        );
        // the gain of 20,000 covers the loss of 20,000 exactly
        assert_eq!(answer.rows[1].absorbed_by_surplus.to_fixed(2), "20000.00");
        assert_eq!(answer.rows[1].charged_to_pnl.to_fixed(0), "0");
        assert_eq!(answer.rows[1].treatment, "mixed");
        assert_eq!(answer.surplus.to_fixed(0), "0");
        assert_eq!(answer.profit_or_loss_impact.to_fixed(0), "0");
    }

    #[test]
    fn a_loss_with_no_surplus_standing_goes_wholly_to_profit_or_loss() {
        let answer = revaluation(&[asset("b", 50_000.0, 0.0)], &[fair("b", 30_000.0)]);
        assert_eq!(answer.rows[0].treatment, "to_profit_or_loss");
        assert_eq!(answer.profit_or_loss_impact.to_fixed(2), "20000.00");
        assert_eq!(answer.surplus_oci.to_fixed(0), "0");
    }

    #[test]
    fn an_asset_with_no_fair_value_is_measured_at_cost() {
        let answer = revaluation(&[asset("a", 100_000.0, 30_000.0)], &[]);
        assert_eq!(answer.total_gain.to_fixed(2), "30000.00");
    }

    #[test]
    fn depreciation_is_adjusted_only_for_an_asset_written_down_and_counts_from_two_years_held() {
        let down = revaluation(&[asset("b", 50_000.0, 0.0)], &[fair("b", 30_000.0)]);
        let adjustment = down.rows[0]
            .depreciation_adjustment
            .as_ref()
            .expect("adjusted");
        // a six-year asset held two years has four left, so 30,000 over four
        assert_eq!(adjustment.remaining_useful_life, 4);
        assert_eq!(adjustment.new_annual_depreciation.to_fixed(2), "7500.00");

        let up = revaluation(&[asset("a", 100_000.0, 20_000.0)], &[fair("a", 120_000.0)]);
        assert!(up.rows[0].depreciation_adjustment.is_none());
    }

    #[test]
    fn a_rate_over_nothing_is_nil_rather_than_a_failure() {
        let answer = revaluation(&[asset("c", 0.0, 0.0)], &[fair("c", -1.0)]);
        let adjustment = answer.rows[0]
            .depreciation_adjustment
            .as_ref()
            .expect("adjusted");
        // the carrying amount was nil, so the old rate has no base to sit on
        assert!(adjustment.current_depreciation_rate.is_zero());
    }

    #[test]
    fn the_report_carries_gains_losses_adjustments_and_the_figures_after() {
        let answer = revalue_from_json(&json!({
            "company_id": "c-1", "revaluation_date": "2026-03-31",
            "assets": [{"asset_id": "a", "asset_name": "Plant", "original_cost": 100000,
                        "accumulated_depreciation": 20000, "residual_value": 0, "useful_life_years": 6}],
            "fair_values": {"a": 135000}
        }))
        .expect("revalued");
        assert_eq!(answer["gains"].as_array().unwrap().len(), 1);
        assert!(answer["losses"].as_array().unwrap().is_empty());
        assert_eq!(answer["total_revaluation_gain"], json!("55000.000000"));
        assert_eq!(answer["net_revaluation_effect"], json!("55000.000000"));
        assert_eq!(answer["new_carrying_amounts"]["a"], json!("135000.000000"));
    }

    #[test]
    fn the_surplus_after_deferred_tax_is_the_gain_less_the_tax_on_it() {
        let answer = surplus_from_json(&json!({
            "revaluation_gain": 50000, "deferred_tax_rate": 0.25, "previous_surplus": 10000
        }))
        .expect("answered");
        assert_eq!(answer["deferred_tax_liability"], json!("12500.000000"));
        assert_eq!(answer["revaluation_surplus_OCI"], json!("37500.000000"));
        assert_eq!(answer["total_revaluation_surplus"], json!("47500.000000"));
    }

    #[test]
    fn the_depreciation_after_revaluation_spreads_the_depreciable_amount_over_what_is_left() {
        let answer = depreciation_after_from_json(&json!({
            "revalued_amount": 120000, "residual_value": 20000,
            "remaining_useful_life": 4, "total_useful_life": 10
        }))
        .expect("answered");
        assert_eq!(answer["depreciable_amount"], json!("100000.000000"));
        assert_eq!(answer["annual_depreciation"], json!("25000.000000"));
        assert_eq!(answer["monthly_depreciation"], json!("2083.333333"));
        // six of ten years gone
        assert_eq!(answer["total_life_expired"], json!("60.000000"));

        let spent = depreciation_after_from_json(&json!({
            "revalued_amount": 120000, "residual_value": 20000,
            "remaining_useful_life": 0, "total_useful_life": 0
        }))
        .expect("answered");
        assert_eq!(spent["annual_depreciation"], json!("0.000000"));
    }

    #[test]
    fn an_impairment_takes_the_surplus_standing_first_and_flags_nothing_to_do_when_there_is_no_loss(
    ) {
        let with_loss = impairment_after_from_json(&json!({
            "carrying_amount": 100000, "recoverable_amount": 60000, "revaluation_surplus_available": 25000
        }))
        .expect("answered");
        assert_eq!(
            with_loss["absorbed_from_revaluation_surplus"],
            json!("25000.000000")
        );
        assert_eq!(
            with_loss["charged_to_profit_or_loss"],
            json!("15000.000000")
        );
        assert_eq!(with_loss["remaining_surplus"], json!("0.000000"));

        let no_loss = impairment_after_from_json(&json!({
            "carrying_amount": 60000, "recoverable_amount": 90000, "revaluation_surplus_available": 0
        }))
        .expect("answered");
        assert_eq!(no_loss["no_impairment_required"], json!(true));
        assert!(no_loss.get("impairment_loss").is_some());
    }

    #[test]
    fn the_indexation_allowance_is_the_rise_in_the_index_applied_to_cost() {
        let answer = indexation_from_json(&json!({
            "original_cost": 10000, "original_date": "2020-01-01", "disposal_date": "2026-01-01",
            "cpi_at_acquisition": 1.5, "cpi_at_disposal": 1.8
        }))
        .expect("answered");
        assert_eq!(answer["indexed_cost"], json!("12000.000000"));
        assert_eq!(answer["indexation_allowance"], json!("2000.000000"));

        let no_index = indexation_from_json(&json!({
            "original_cost": 10000, "cpi_at_acquisition": 0, "cpi_at_disposal": 5
        }))
        .expect("answered");
        // nothing to divide by leaves the allowance at nil
        assert_eq!(no_index["indexation_allowance"], json!("0.000000"));
    }
}
