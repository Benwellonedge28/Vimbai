#![forbid(unsafe_code)]
//! Biological assets (IAS 41): fair value less costs to sell, the three fair
//! value levels, the consumable/bearer distinction and the disclosure template.
//!
//! Ported from biological-assets-service. Amounts are `Money` throughout.
//!
//! Kept as shipped:
//!
//! * The market price per unit comes from a fixed table by category and
//!   maturity (5000 / 2500 / 800 / 400, and 1000 for anything unlisted), and
//!   costs to sell are a flat 5% of market price.
//! * Transformation gains split physical from price change by valuing every
//!   unit of change at 500, and the cost change is the costs incurred negated.
//!   A transformation is presented in profit or loss only when its increase
//!   type is `harvest` or `sale`; everything else is OCI.
//! * Harvested amount and agricultural produce amount are the same figure -
//!   quantity times selling price - and costs to sell are summed separately.
//! * The disclosure template always reports every asset as level 3.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vimbai_common::Money;

fn zero() -> Money {
    Money::zero()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BiologicalAsset {
    pub asset_id: String,
    #[serde(default)]
    pub asset_name: String,
    #[serde(default)]
    pub asset_category: String,
    #[serde(default)]
    pub species: String,
    pub quantity: Money,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub age_years: f64,
    #[serde(default)]
    pub maturity_status: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub acquisition_date: Option<String>,
    #[serde(default)]
    pub initial_cost: Option<Money>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransformationEntry {
    #[serde(default)]
    pub entry_date: String,
    #[serde(default)]
    pub increase_type: String,
    #[serde(default = "zero")]
    pub quantity_change: Money,
    #[serde(default = "zero")]
    pub fair_value_change: Money,
    #[serde(default = "zero")]
    pub costs_incurred: Money,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarvestEntry {
    #[serde(default)]
    pub harvest_date: String,
    #[serde(default = "zero")]
    pub quantity_harvested: Money,
    #[serde(default = "zero")]
    pub selling_price_per_unit: Money,
    #[serde(default = "zero")]
    pub costs_to_sell: Money,
    #[serde(default = "zero")]
    pub carrying_amount_at_harvest: Money,
    #[serde(default = "zero")]
    pub gain_loss_on_harvest: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BiologicalAssetsRequest {
    pub company_id: String,
    #[serde(default)]
    pub reporting_date: String,
    #[serde(default)]
    pub assets: Vec<BiologicalAsset>,
    #[serde(default)]
    pub transformations: Vec<TransformationEntry>,
    #[serde(default)]
    pub harvests: Vec<HarvestEntry>,
    #[serde(default = "default_true")]
    pub include_disclosure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetFairValueMeasurement {
    pub asset_id: String,
    pub asset_name: String,
    pub category: String,
    pub quantity: Money,
    pub unit: String,
    pub age: f64,
    pub maturity: String,
    pub market_price_per_unit: Money,
    pub estimated_costs_to_sell: Money,
    pub fair_value_per_unit: Money,
    pub total_fair_value: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GainLossBreakdown {
    pub asset_id: String,
    pub physical_change: Money,
    pub price_change: Money,
    pub cost_change: Money,
    pub total_gain_loss: Money,
    pub recognized_in_pnl: Money,
    pub recognized_in_oci: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BiologicalAssetsResponse {
    pub company_id: String,
    pub reporting_date: String,
    pub maturity_analysis: BTreeMap<String, usize>,
    pub fair_value_measurements: Vec<AssetFairValueMeasurement>,
    pub gain_loss_breakdown: Vec<GainLossBreakdown>,
    pub total_biological_assets: Money,
    pub biological_asset_change: Money,
    pub harvested_amount: Money,
    pub agricultural_produce_amount: Money,
    pub costs_to_sell_total: Money,
}

/// The service's own price table, and the 1000 it fell back to.
fn market_price(category: &str, maturity: &str) -> Money {
    let amount = match (category, maturity) {
        ("bearer_biological", "mature") => 5000,
        ("bearer_biological", "immature") => 2500,
        ("consumable_biological", "mature") => 800,
        ("consumable_biological", "immature") => 400,
        _ => 1000,
    };
    Money::from_i64(amount)
}

pub fn measure_biological_assets(req: &BiologicalAssetsRequest) -> BiologicalAssetsResponse {
    let mut maturity_analysis: BTreeMap<String, usize> = BTreeMap::new();
    let mut fair_value_measurements = Vec::new();
    let mut total_fair_value = Money::zero();

    for asset in &req.assets {
        *maturity_analysis
            .entry(asset.maturity_status.clone())
            .or_insert(0) += 1;

        let price = market_price(&asset.asset_category, &asset.maturity_status);
        // 5% of market price, charged before the quantity is applied
        let costs_per_unit = price.times(&Money::from_i64(5)).over(&Money::from_i64(100));
        let fair_value_per_unit = price.minus(&costs_per_unit);
        let total = fair_value_per_unit.times(&asset.quantity);
        total_fair_value = total_fair_value.plus(&total);

        fair_value_measurements.push(AssetFairValueMeasurement {
            asset_id: asset.asset_id.clone(),
            asset_name: asset.asset_name.clone(),
            category: asset.asset_category.clone(),
            quantity: asset.quantity.clone(),
            unit: asset.unit.clone(),
            age: asset.age_years,
            maturity: asset.maturity_status.clone(),
            market_price_per_unit: price.clone(),
            estimated_costs_to_sell: costs_per_unit.times(&asset.quantity).round(2),
            fair_value_per_unit: fair_value_per_unit.round(2),
            total_fair_value: total.round(2),
        });
    }

    let mut gain_loss_breakdown = Vec::new();
    let mut total_gain = Money::zero();

    for trans in &req.transformations {
        let physical_change = trans.quantity_change.times(&Money::from_i64(500));
        let price_change = trans.fair_value_change.minus(&physical_change);
        let total_loss = trans.fair_value_change.clone();
        let to_pnl = trans.increase_type == "harvest" || trans.increase_type == "sale";

        gain_loss_breakdown.push(GainLossBreakdown {
            asset_id: "TBD".to_string(),
            physical_change: physical_change.round(2),
            price_change: price_change.round(2),
            cost_change: trans.costs_incurred.negate().round(2),
            total_gain_loss: total_loss.round(2),
            recognized_in_pnl: if to_pnl {
                total_loss.round(2)
            } else {
                Money::zero()
            },
            recognized_in_oci: if to_pnl {
                Money::zero()
            } else {
                total_loss.round(2)
            },
        });
        total_gain = total_gain.plus(&total_loss);
    }

    let harvested = Money::sum(
        req.harvests
            .iter()
            .map(|h| h.quantity_harvested.times(&h.selling_price_per_unit))
            .collect::<Vec<_>>()
            .iter(),
    );
    let costs_to_sell = Money::sum(req.harvests.iter().map(|h| &h.costs_to_sell));

    BiologicalAssetsResponse {
        company_id: req.company_id.clone(),
        reporting_date: req.reporting_date.clone(),
        maturity_analysis,
        fair_value_measurements,
        gain_loss_breakdown,
        total_biological_assets: total_fair_value.round(2),
        biological_asset_change: total_gain.round(2),
        harvested_amount: harvested.round(2),
        agricultural_produce_amount: harvested.round(2),
        costs_to_sell_total: costs_to_sell.round(2),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Level1Request {
    pub asset_quantity: Money,
    pub quoted_price_per_unit: Money,
    #[serde(default)]
    pub costs_to_sell_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Level1Result {
    pub level: u8,
    pub measurement_technique: String,
    pub asset_quantity: Money,
    pub quoted_price_per_unit: Money,
    pub costs_to_sell: Money,
    pub fair_value: Money,
    pub fair_value_per_unit: Money,
}

pub fn measure_level1_fair_value(req: &Level1Request) -> Level1Result {
    let gross = req.quoted_price_per_unit.times(&req.asset_quantity);
    let costs_to_sell = gross.times(&rate(req.costs_to_sell_percentage)).round(2);
    Level1Result {
        level: 1,
        measurement_technique: "Quoted price in active market".to_string(),
        asset_quantity: req.asset_quantity.clone(),
        quoted_price_per_unit: req.quoted_price_per_unit.round(2),
        costs_to_sell: costs_to_sell.clone(),
        fair_value: gross.minus(&costs_to_sell).round(2),
        fair_value_per_unit: req
            .quoted_price_per_unit
            .minus(
                &req.quoted_price_per_unit
                    .times(&rate(req.costs_to_sell_percentage)),
            )
            .round(2),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Level2Request {
    pub asset_quantity: Money,
    pub market_price_similar: Money,
    #[serde(default)]
    pub adjustment_factor: f64,
    #[serde(default)]
    pub costs_to_sell_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Level2Result {
    pub level: u8,
    pub measurement_technique: String,
    pub asset_quantity: Money,
    pub market_price_similar: Money,
    pub adjustment_factor: f64,
    pub adjusted_price: Money,
    pub costs_to_sell: Money,
    pub fair_value: Money,
}

pub fn measure_level2_fair_value(req: &Level2Request) -> Level2Result {
    let adjusted = req
        .market_price_similar
        .times(&rate(req.adjustment_factor))
        .round(2);
    let gross = adjusted.times(&req.asset_quantity);
    let costs_to_sell = gross.times(&rate(req.costs_to_sell_percentage)).round(2);
    Level2Result {
        level: 2,
        measurement_technique: "Market-comparable prices".to_string(),
        asset_quantity: req.asset_quantity.clone(),
        market_price_similar: req.market_price_similar.round(2),
        adjustment_factor: req.adjustment_factor,
        adjusted_price: adjusted,
        costs_to_sell: costs_to_sell.clone(),
        fair_value: gross.minus(&costs_to_sell).round(2),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Level3Request {
    #[serde(default = "zero")]
    pub present_value_fcfe: Money,
    #[serde(default = "zero")]
    pub terminal_value: Money,
    #[serde(default = "zero")]
    pub costs_to_sell: Money,
    #[serde(default)]
    pub discount_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Level3Result {
    pub level: u8,
    pub measurement_technique: String,
    pub present_value_fcfe: Money,
    pub terminal_value: Money,
    pub costs_to_sell: Money,
    pub fair_value: Money,
    pub discount_rate: f64,
    pub key_unobservable_inputs: Vec<String>,
}

pub fn measure_level3_fair_value(req: &Level3Request) -> Level3Result {
    let total = req.present_value_fcfe.plus(&req.terminal_value);
    Level3Result {
        level: 3,
        measurement_technique: "Discounted cash flows".to_string(),
        present_value_fcfe: req.present_value_fcfe.round(2),
        terminal_value: req.terminal_value.round(2),
        costs_to_sell: req.costs_to_sell.round(2),
        fair_value: total.minus(&req.costs_to_sell).round(2),
        discount_rate: req.discount_rate,
        key_unobservable_inputs: vec![
            "cash flow projections".to_string(),
            "terminal growth rate".to_string(),
            "discount rate".to_string(),
        ],
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClassificationRequest {
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub reproduction_purpose: String,
    #[serde(default)]
    pub harvesting_intent: String,
    #[serde(default)]
    pub historical_use: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClassificationResult {
    pub classification: String,
    pub description: String,
    pub accounting_treatment: String,
}

pub fn classify_biological_asset(req: &ClassificationRequest) -> ClassificationResult {
    let consumable = req.purpose == "sale"
        || req.harvesting_intent == "harvest"
        || req.historical_use == "harvest";
    if consumable {
        ClassificationResult {
            classification: "consumable".to_string(),
            description: "Biological asset to be harvested as agricultural produce or sold as biological asset"
                .to_string(),
            accounting_treatment: "Recognized at fair value less costs to sell".to_string(),
        }
    } else {
        ClassificationResult {
            classification: "bearer".to_string(),
            description:
                "Biological asset used to produce agricultural produce over multiple periods"
                    .to_string(),
            accounting_treatment: "Bearer biological assets also at fair value less costs to sell"
                .to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisclosureRequest {
    #[serde(default)]
    pub assets: Vec<BiologicalAsset>,
    #[serde(default)]
    pub reporting_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisclosureResult {
    pub reporting_date: String,
    pub disclosure_sections: Vec<String>,
    pub assets_by_category: BTreeMap<String, usize>,
    pub assets_by_location: BTreeMap<String, usize>,
    pub fair_value_hierarchy_required: bool,
    pub level_1_assets: usize,
    pub level_2_assets: usize,
    pub level_3_assets: usize,
}

pub fn prepare_disclosure(req: &DisclosureRequest) -> DisclosureResult {
    let mut by_category: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_location: BTreeMap<String, usize> = BTreeMap::new();
    for asset in &req.assets {
        *by_category.entry(asset.asset_category.clone()).or_insert(0) += 1;
        *by_location.entry(asset.location.clone()).or_insert(0) += 1;
    }

    DisclosureResult {
        reporting_date: req.reporting_date.clone(),
        disclosure_sections: [
            "Nature of activities",
            "Measurement basis (fair value less costs to sell)",
            "Financial risk management",
            "Quantitative disclosures by category",
            "Quantitative disclosures by location",
            "Gains/losses on fair value changes",
            "New assets acquired/developed",
            "Assets sold or harvested",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        assets_by_category: by_category,
        assets_by_location: by_location,
        fair_value_hierarchy_required: true,
        level_1_assets: 0,
        level_2_assets: 0,
        level_3_assets: req.assets.len(),
    }
}

/// A rate the caller supplied as a fraction. A rate that cannot be read is
/// treated as zero rather than as a silent 1.0, so a typo never doubles a figure.
fn rate(value: f64) -> Money {
    Money::from_f64_shortest(value).unwrap_or_else(|_| Money::zero())
}

pub fn measure_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: BiologicalAssetsRequest = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(measure_biological_assets(&request)).map_err(|error| error.to_string())
}

pub fn level1_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: Level1Request = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(measure_level1_fair_value(&request)).map_err(|error| error.to_string())
}

pub fn level2_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: Level2Request = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(measure_level2_fair_value(&request)).map_err(|error| error.to_string())
}

pub fn level3_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: Level3Request = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(measure_level3_fair_value(&request)).map_err(|error| error.to_string())
}

pub fn biological_classify_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request: ClassificationRequest = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(classify_biological_asset(&request)).map_err(|error| error.to_string())
}

pub fn disclosure_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: DisclosureRequest = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(prepare_disclosure(&request)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    fn asset(id: &str, category: &str, maturity: &str, quantity: &str) -> BiologicalAsset {
        BiologicalAsset {
            asset_id: id.to_string(),
            asset_name: format!("Herd {id}"),
            asset_category: category.to_string(),
            species: "cattle".to_string(),
            quantity: m(quantity),
            unit: "head".to_string(),
            age_years: 3.0,
            maturity_status: maturity.to_string(),
            location: "Farm A".to_string(),
            acquisition_date: None,
            initial_cost: None,
        }
    }

    #[test]
    fn the_price_table_and_five_percent_cost_to_sell_drive_the_measurement() {
        let req = BiologicalAssetsRequest {
            company_id: "C1".to_string(),
            reporting_date: "2026-10-31".to_string(),
            assets: vec![
                asset("A1", "bearer_biological", "mature", "10"),
                asset("A2", "consumable_biological", "immature", "100"),
                asset("A3", "exotic", "mature", "1"),
            ],
            transformations: Vec::new(),
            harvests: Vec::new(),
            include_disclosure: true,
        };
        let out = measure_biological_assets(&req);

        assert_eq!(out.maturity_analysis.get("mature"), Some(&2));
        assert_eq!(out.maturity_analysis.get("immature"), Some(&1));
        // 5000 less 5% = 4750 a head, ten head
        assert_eq!(
            out.fair_value_measurements[0]
                .market_price_per_unit
                .to_fixed(2),
            "5000.00"
        );
        assert_eq!(
            out.fair_value_measurements[0]
                .fair_value_per_unit
                .to_fixed(2),
            "4750.00"
        );
        assert_eq!(
            out.fair_value_measurements[0].total_fair_value.to_fixed(2),
            "47500.00"
        );
        assert_eq!(
            out.fair_value_measurements[0]
                .estimated_costs_to_sell
                .to_fixed(2),
            "2500.00"
        );
        // an unlisted category falls back to 1000
        assert_eq!(
            out.fair_value_measurements[2]
                .market_price_per_unit
                .to_fixed(2),
            "1000.00"
        );
        // 47500 + 38000 + 950
        assert_eq!(out.total_biological_assets.to_fixed(2), "86450.00");
    }

    #[test]
    fn transformations_split_physical_from_price_and_present_by_increase_type() {
        let req = BiologicalAssetsRequest {
            company_id: "C1".to_string(),
            reporting_date: "2026-10-31".to_string(),
            assets: Vec::new(),
            transformations: vec![
                TransformationEntry {
                    entry_date: "2026-09-30".to_string(),
                    increase_type: "growth".to_string(),
                    quantity_change: m("4"),
                    fair_value_change: m("3000"),
                    costs_incurred: m("200"),
                    description: "weight gain".to_string(),
                },
                TransformationEntry {
                    entry_date: "2026-10-15".to_string(),
                    increase_type: "sale".to_string(),
                    quantity_change: m("2"),
                    fair_value_change: m("1000"),
                    costs_incurred: m("50"),
                    description: "sold".to_string(),
                },
            ],
            harvests: vec![HarvestEntry {
                harvest_date: "2026-10-20".to_string(),
                quantity_harvested: m("500"),
                selling_price_per_unit: m("12.50"),
                costs_to_sell: m("300"),
                carrying_amount_at_harvest: m("5000"),
                gain_loss_on_harvest: m("0"),
            }],
            include_disclosure: true,
        };
        let out = measure_biological_assets(&req);

        // growth: 4 head at the fixed 500 a head
        assert_eq!(
            out.gain_loss_breakdown[0].physical_change.to_fixed(2),
            "2000.00"
        );
        assert_eq!(
            out.gain_loss_breakdown[0].price_change.to_fixed(2),
            "1000.00"
        );
        assert_eq!(
            out.gain_loss_breakdown[0].cost_change.to_fixed(2),
            "-200.00"
        );
        assert_eq!(
            out.gain_loss_breakdown[0].recognized_in_oci.to_fixed(2),
            "3000.00"
        );
        assert_eq!(out.gain_loss_breakdown[0].recognized_in_pnl, Money::zero());
        // sale goes to profit or loss instead
        assert_eq!(
            out.gain_loss_breakdown[1].recognized_in_pnl.to_fixed(2),
            "1000.00"
        );
        assert_eq!(out.gain_loss_breakdown[1].recognized_in_oci, Money::zero());
        assert_eq!(out.biological_asset_change.to_fixed(2), "4000.00");
        // harvested and agricultural produce are the same figure
        assert_eq!(out.harvested_amount.to_fixed(2), "6250.00");
        assert_eq!(out.agricultural_produce_amount.to_fixed(2), "6250.00");
        assert_eq!(out.costs_to_sell_total.to_fixed(2), "300.00");
    }

    #[test]
    fn the_three_levels_are_arithmetic_on_the_callers_figures() {
        let one = measure_level1_fair_value(&Level1Request {
            asset_quantity: m("100"),
            quoted_price_per_unit: m("12.50"),
            costs_to_sell_percentage: 0.05,
        });
        assert_eq!(one.costs_to_sell.to_fixed(2), "62.50");
        assert_eq!(one.fair_value.to_fixed(2), "1187.50");
        assert_eq!(one.fair_value_per_unit.to_fixed(2), "11.88");

        let two = measure_level2_fair_value(&Level2Request {
            asset_quantity: m("10"),
            market_price_similar: m("1000"),
            adjustment_factor: 0.9,
            costs_to_sell_percentage: 0.1,
        });
        assert_eq!(two.adjusted_price.to_fixed(2), "900.00");
        assert_eq!(two.costs_to_sell.to_fixed(2), "900.00");
        assert_eq!(two.fair_value.to_fixed(2), "8100.00");

        let three = measure_level3_fair_value(&Level3Request {
            present_value_fcfe: m("90000"),
            terminal_value: m("15000"),
            costs_to_sell: m("500"),
            discount_rate: 0.12,
        });
        assert_eq!(three.fair_value.to_fixed(2), "104500.00");
        assert_eq!(three.key_unobservable_inputs.len(), 3);
    }

    #[test]
    fn classification_follows_intent_and_use() {
        let sold = classify_biological_asset(&ClassificationRequest {
            purpose: "sale".to_string(),
            reproduction_purpose: String::new(),
            harvesting_intent: String::new(),
            historical_use: String::new(),
        });
        assert_eq!(sold.classification, "consumable");

        let harvested = classify_biological_asset(&ClassificationRequest {
            purpose: "production".to_string(),
            reproduction_purpose: "milk".to_string(),
            harvesting_intent: String::new(),
            historical_use: "harvest".to_string(),
        });
        assert_eq!(harvested.classification, "consumable");

        let bearer = classify_biological_asset(&ClassificationRequest {
            purpose: "production".to_string(),
            reproduction_purpose: "milk".to_string(),
            harvesting_intent: String::new(),
            historical_use: "milking".to_string(),
        });
        assert_eq!(bearer.classification, "bearer");
        assert_eq!(
            bearer.accounting_treatment,
            "Bearer biological assets also at fair value less costs to sell"
        );
    }

    #[test]
    fn the_disclosure_template_calls_every_asset_level_three() {
        let out = prepare_disclosure(&DisclosureRequest {
            assets: vec![
                asset("A1", "bearer_biological", "mature", "10"),
                asset("A2", "bearer_biological", "immature", "5"),
                asset("A3", "consumable_biological", "mature", "100"),
            ],
            reporting_date: "2026-10-31".to_string(),
        });
        assert_eq!(out.level_1_assets, 0);
        assert_eq!(out.level_2_assets, 0);
        assert_eq!(out.level_3_assets, 3);
        assert_eq!(out.assets_by_category.get("bearer_biological"), Some(&2));
        assert_eq!(out.assets_by_location.get("Farm A"), Some(&3));
        assert_eq!(out.disclosure_sections.len(), 8);
    }

    #[test]
    fn quantities_past_double_precision_keep_their_digits() {
        let req = BiologicalAssetsRequest {
            company_id: "C1".to_string(),
            reporting_date: "2026-10-31".to_string(),
            assets: vec![asset(
                "A1",
                "bearer_biological",
                "mature",
                "123456789012345678901.55",
            )],
            transformations: Vec::new(),
            harvests: Vec::new(),
            include_disclosure: true,
        };
        let out = measure_biological_assets(&req);
        // 4750 a head against a 20-digit herd
        assert_eq!(
            out.total_biological_assets.to_fixed(2),
            "586419747808641974782362.50"
        );
    }
}
