#![forbid(unsafe_code)]
//! Investment property (IAS 40): the fair value model, the cost model, a
//! transfer between owner-occupied and investment property, the fair value
//! hierarchy disclosure and property yields.
//!
//! Ported from investment-property-service. Every amount is `Money`, so a
//! portfolio past double precision reports exactly.
//!
//! Kept as shipped, because these are the numbers the service published:
//!
//! * `additions` and `disposals` are always zero - the service never accepted
//!   them, so the movement in fair value is opening to closing and nothing else.
//! * Rental income and direct operating expenses are allocated across the
//!   portfolio in proportion to each property's OPENING fair value over the
//!   whole portfolio's opening fair value, the same ratio for every property.
//! * Under the fair value model the whole gain is presented in OCI and the
//!   profit-or-loss line is zero.
//! * Under the cost model the opening accumulated depreciation is twice the
//!   annual charge ("assume 2 years old") and every property is modelled as
//!   disposed of at 1.1x cost, so a gain on disposal is always reported.
//! * Disclosure tiers are by size alone: over 10,000,000 is level 3, over
//!   1,000,000 is level 2, everything else is level 1.
//! * Yields charge a flat 30% of rent as operating expenses.
//!
//! Fixed: a zero total opening fair value (or a zero useful life, or a zero
//! capitalisation rate) reported nothing where the service divided by zero and
//! raised. Each now reports zero.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vimbai_common::{py_round, Money};

fn zero() -> Money {
    Money::zero()
}

fn default_life() -> i64 {
    50
}

fn default_method() -> String {
    "straight_line".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvestmentProperty {
    pub property_id: String,
    #[serde(default)]
    pub property_name: String,
    #[serde(default)]
    pub property_type: String,
    #[serde(default)]
    pub acquisition_date: String,
    pub acquisition_cost: Money,
    pub fair_value_last_reporting: Money,
    #[serde(default = "default_life")]
    pub useful_life_years: i64,
    #[serde(default = "zero")]
    pub residual_value: Money,
    #[serde(default = "default_method")]
    pub depreciation_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FairValueModelRequest {
    pub company_id: String,
    #[serde(default)]
    pub reporting_date: String,
    #[serde(default)]
    pub properties: Vec<InvestmentProperty>,
    #[serde(default)]
    pub current_fair_values: BTreeMap<String, Money>,
    #[serde(default = "zero")]
    pub rental_income: Money,
    #[serde(default = "zero")]
    pub direct_operating_expenses: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostModelRequest {
    pub company_id: String,
    #[serde(default)]
    pub reporting_date: String,
    #[serde(default)]
    pub properties: Vec<InvestmentProperty>,
    #[serde(default)]
    pub depreciation_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FairValueMeasurement {
    pub property_id: String,
    pub property_name: String,
    pub opening_fair_value: Money,
    pub additions: Money,
    pub disposals: Money,
    pub fair_value_gain_loss: Money,
    pub closing_fair_value: Money,
    pub rental_income: Money,
    pub direct_expenses: Money,
    pub net_income: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostModelDepreciation {
    pub property_id: String,
    pub property_name: String,
    pub cost: Money,
    pub accumulated_depreciation_opening: Money,
    pub depreciation_for_period: Money,
    pub accumulated_depreciation_closing: Money,
    pub disposal_proceeds: Money,
    pub gain_loss_on_disposal: Money,
    pub carrying_amount: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvestmentPropertyResponse {
    pub company_id: String,
    pub reporting_date: String,
    pub accounting_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fair_value_measurements: Option<Vec<FairValueMeasurement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_model_depreciations: Option<Vec<CostModelDepreciation>>,
    pub total_fair_value_or_carrying_amount: Money,
    pub fair_value_gain_loss_in_statement_of_profit_or_loss: Money,
    pub fair_value_gain_loss_in_oci: Money,
    pub direct_expenses_including_depreciation: Money,
}

fn opening_total(properties: &[InvestmentProperty]) -> Money {
    Money::sum(properties.iter().map(|p| &p.fair_value_last_reporting))
}

/// One property's share of a portfolio-wide amount, in proportion to its
/// opening fair value. A zero portfolio allocates nothing instead of raising.
fn allocate_by_opening(amount: &Money, opening: &Money, total: &Money) -> Money {
    if total.is_zero() {
        return Money::zero();
    }
    amount.times(&opening.over(total)).round(2)
}

pub fn apply_fair_value_model(req: &FairValueModelRequest) -> InvestmentPropertyResponse {
    let total_opening = opening_total(&req.properties);
    let mut fair_value_measurements = Vec::new();
    let mut total_fair_value = Money::zero();
    let mut total_gain = Money::zero();

    for prop in &req.properties {
        let current = req
            .current_fair_values
            .get(&prop.property_id)
            .cloned()
            .unwrap_or_else(|| prop.fair_value_last_reporting.clone());

        let opening = &prop.fair_value_last_reporting;
        let gain = current.minus(opening);
        total_fair_value = total_fair_value.plus(&current);
        total_gain = total_gain.plus(&gain);

        let rental = allocate_by_opening(&req.rental_income, opening, &total_opening);
        let expenses = allocate_by_opening(&req.direct_operating_expenses, opening, &total_opening);

        fair_value_measurements.push(FairValueMeasurement {
            property_id: prop.property_id.clone(),
            property_name: prop.property_name.clone(),
            opening_fair_value: opening.round(2),
            additions: Money::zero(),
            disposals: Money::zero(),
            fair_value_gain_loss: gain.round(2),
            closing_fair_value: current.round(2),
            rental_income: rental.clone(),
            direct_expenses: expenses.clone(),
            net_income: rental.minus(&expenses).round(2),
        });
    }

    InvestmentPropertyResponse {
        company_id: req.company_id.clone(),
        reporting_date: req.reporting_date.clone(),
        accounting_model: "fair_value".to_string(),
        fair_value_measurements: Some(fair_value_measurements),
        cost_model_depreciations: None,
        total_fair_value_or_carrying_amount: total_fair_value.round(2),
        fair_value_gain_loss_in_statement_of_profit_or_loss: Money::zero(),
        fair_value_gain_loss_in_oci: total_gain.round(2),
        direct_expenses_including_depreciation: req.direct_operating_expenses.round(2),
    }
}

pub fn apply_cost_model(req: &CostModelRequest) -> InvestmentPropertyResponse {
    let mut rows = Vec::new();
    let mut total_carrying = Money::zero();

    for prop in &req.properties {
        let annual = if prop.useful_life_years == 0 {
            Money::zero()
        } else {
            prop.acquisition_cost
                .minus(&prop.residual_value)
                .over(&Money::from_i64(prop.useful_life_years))
        };
        // the service modelled every property as two years old
        let opening_accumulated = annual.times(&Money::from_i64(2));
        let closing_accumulated = opening_accumulated.plus(&annual);
        // ... and as disposed of at 1.1x cost
        let proceeds = prop
            .acquisition_cost
            .times(&Money::from_i64(11))
            .over(&Money::from_i64(10));
        let carrying = prop.acquisition_cost.minus(&closing_accumulated);
        let gain = proceeds.minus(&carrying);
        total_carrying = total_carrying.plus(&carrying);

        rows.push(CostModelDepreciation {
            property_id: prop.property_id.clone(),
            property_name: prop.property_name.clone(),
            cost: prop.acquisition_cost.round(2),
            accumulated_depreciation_opening: opening_accumulated.round(2),
            depreciation_for_period: annual.round(2),
            accumulated_depreciation_closing: closing_accumulated.round(2),
            disposal_proceeds: proceeds.round(2),
            gain_loss_on_disposal: gain.round(2),
            carrying_amount: carrying.round(2),
        });
    }

    let period_total = Money::sum(rows.iter().map(|r| &r.depreciation_for_period));

    InvestmentPropertyResponse {
        company_id: req.company_id.clone(),
        reporting_date: req.reporting_date.clone(),
        accounting_model: "cost".to_string(),
        fair_value_measurements: None,
        cost_model_depreciations: Some(rows),
        total_fair_value_or_carrying_amount: total_carrying.round(2),
        fair_value_gain_loss_in_statement_of_profit_or_loss: Money::zero(),
        fair_value_gain_loss_in_oci: Money::zero(),
        direct_expenses_including_depreciation: period_total,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransferRequest {
    pub property_id: String,
    #[serde(default)]
    pub reason_for_transfer: String,
    pub carrying_amount: Money,
    pub fair_value: Money,
    pub model_from: String,
    pub model_to: String,
    /// The service stamped the day it was called; a route has no clock, so the
    /// caller supplies the date it is reporting.
    #[serde(default)]
    pub transfer_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransferResult {
    pub property_id: String,
    pub transfer_date: String,
    pub reason: String,
    pub carrying_amount: Money,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fair_value_at_transfer: Option<Money>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adjustment_to_property: Option<Money>,
    pub treatment: String,
}

pub fn process_transfer(req: &TransferRequest) -> TransferResult {
    if req.model_from == "owner_occupied" && req.model_to == "investment" {
        let adjustment = req.fair_value.minus(&req.carrying_amount);
        TransferResult {
            property_id: req.property_id.clone(),
            transfer_date: req.transfer_date.clone(),
            reason: req.reason_for_transfer.clone(),
            carrying_amount: req.carrying_amount.round(2),
            fair_value_at_transfer: Some(req.fair_value.round(2)),
            adjustment_to_property: Some(adjustment.round(2)),
            treatment: if adjustment.to_f64_lossy() > 0.0 {
                "recognized in OCI".to_string()
            } else {
                "recognized in P&L".to_string()
            },
        }
    } else {
        TransferResult {
            property_id: req.property_id.clone(),
            transfer_date: req.transfer_date.clone(),
            reason: req.reason_for_transfer.clone(),
            carrying_amount: req.carrying_amount.round(2),
            fair_value_at_transfer: None,
            adjustment_to_property: None,
            treatment: "cost becomes deemed cost for subsequent measurement".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisclosureRequest {
    #[serde(default)]
    pub properties: Vec<InvestmentProperty>,
    #[serde(default)]
    pub current_fair_values: BTreeMap<String, Money>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisclosureResult {
    pub total_properties: usize,
    pub level_1_fair_value: Money,
    pub level_2_fair_value: Money,
    pub level_3_fair_value: Money,
    pub level_1_count: usize,
    pub level_2_count: usize,
    pub level_3_count: usize,
    pub valuation_technique: String,
    pub significant_unobservable_inputs: Vec<String>,
}

pub fn prepare_fair_value_disclosure(req: &DisclosureRequest) -> DisclosureResult {
    let mut levels: [Vec<Money>; 3] = [Vec::new(), Vec::new(), Vec::new()];

    for prop in &req.properties {
        let fair_value = req
            .current_fair_values
            .get(&prop.property_id)
            .cloned()
            .unwrap_or_else(|| prop.fair_value_last_reporting.clone());
        let tier = if fair_value.to_f64_lossy() > 10_000_000.0 {
            2
        } else if fair_value.to_f64_lossy() > 1_000_000.0 {
            1
        } else {
            0
        };
        levels[tier].push(fair_value);
    }

    let has_level_3 = !levels[2].is_empty();

    DisclosureResult {
        total_properties: req.properties.len(),
        level_1_fair_value: Money::sum(levels[0].iter()).round(2),
        level_2_fair_value: Money::sum(levels[1].iter()).round(2),
        level_3_fair_value: Money::sum(levels[2].iter()).round(2),
        level_1_count: levels[0].len(),
        level_2_count: levels[1].len(),
        level_3_count: levels[2].len(),
        valuation_technique: if has_level_3 {
            "Discounted cash flows".to_string()
        } else {
            "Market comparison".to_string()
        },
        significant_unobservable_inputs: if has_level_3 {
            vec![
                "DCF rate".to_string(),
                "Occupancy rate".to_string(),
                "Growth rate".to_string(),
            ]
        } else {
            Vec::new()
        },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct YieldRequest {
    pub rental_income_annual: Money,
    pub current_fair_value: Money,
    #[serde(default)]
    pub capitalization_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct YieldResult {
    pub rental_income_annual: Money,
    pub current_fair_value: Money,
    pub gross_yield: f64,
    pub net_yield: f64,
    pub capitalization_rate: f64,
    pub market_value_implied: Money,
    pub capital_growth: f64,
}

pub fn calculate_property_yields(req: &YieldRequest) -> YieldResult {
    let gross = ratio_pct(&req.rental_income_annual, &req.current_fair_value, 100.0);
    // the service charges a flat 30% of rent as operating expenses
    let thirty_percent = req
        .rental_income_annual
        .times(&Money::from_i64(3))
        .over(&Money::from_i64(10))
        .round(2);
    let net_income = req.rental_income_annual.minus(&thirty_percent);
    let net = ratio_pct(&net_income, &req.current_fair_value, 100.0);

    let implied = if req.capitalization_rate > 0.0 {
        req.rental_income_annual
            .times(&Money::from_i64(100))
            .over(
                &Money::from_f64_shortest(req.capitalization_rate)
                    .unwrap_or_else(|_| Money::zero()),
            )
            .round(2)
    } else {
        Money::zero()
    };

    let growth = if implied.is_zero() {
        0.0
    } else {
        ratio_pct(&req.current_fair_value.minus(&implied), &implied, 100.0)
    };

    YieldResult {
        rental_income_annual: req.rental_income_annual.round(2),
        current_fair_value: req.current_fair_value.round(2),
        gross_yield: py_round(gross, 2),
        net_yield: py_round(net, 2),
        capitalization_rate: req.capitalization_rate,
        market_value_implied: implied,
        capital_growth: py_round(growth, 2),
    }
}

/// `value/denominator * scale`, with the zero denominator the service raised on
/// reporting 0 instead.
fn ratio_pct(value: &Money, denominator: &Money, scale: f64) -> f64 {
    if denominator.is_zero() {
        return 0.0;
    }
    value.over(denominator).to_f64_lossy() * scale
}

pub fn fair_value_model_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request: FairValueModelRequest = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(apply_fair_value_model(&request)).map_err(|error| error.to_string())
}

pub fn cost_model_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: CostModelRequest = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(apply_cost_model(&request)).map_err(|error| error.to_string())
}

pub fn transfer_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: TransferRequest = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(process_transfer(&request)).map_err(|error| error.to_string())
}

pub fn disclosure_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: DisclosureRequest = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(prepare_fair_value_disclosure(&request)).map_err(|error| error.to_string())
}

pub fn yields_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: YieldRequest = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(calculate_property_yields(&request)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    fn property(id: &str, opening: &str, cost: &str) -> InvestmentProperty {
        InvestmentProperty {
            property_id: id.to_string(),
            property_name: format!("Property {id}"),
            property_type: "building".to_string(),
            acquisition_date: "2020-01-01".to_string(),
            acquisition_cost: m(cost),
            fair_value_last_reporting: m(opening),
            useful_life_years: 50,
            residual_value: Money::zero(),
            depreciation_method: "straight_line".to_string(),
        }
    }

    #[test]
    fn fair_value_movement_is_opening_to_closing_with_nothing_in_between() {
        let req = FairValueModelRequest {
            company_id: "C1".to_string(),
            reporting_date: "2026-10-31".to_string(),
            properties: vec![
                property("P1", "1000", "800"),
                property("P2", "3000", "2600"),
            ],
            current_fair_values: BTreeMap::from([
                ("P1".to_string(), m("1200")),
                ("P2".to_string(), m("3100")),
            ]),
            rental_income: m("800"),
            direct_operating_expenses: m("300"),
        };
        let out = apply_fair_value_model(&req);

        assert_eq!(out.accounting_model, "fair_value");
        assert_eq!(
            out.fair_value_measurements.as_ref().unwrap()[0].additions,
            Money::zero()
        );
        assert_eq!(
            out.fair_value_measurements.as_ref().unwrap()[0].disposals,
            Money::zero()
        );
        assert_eq!(
            out.fair_value_measurements.as_ref().unwrap()[0]
                .fair_value_gain_loss
                .to_fixed(2),
            "200.00"
        );
        assert_eq!(
            out.total_fair_value_or_carrying_amount.to_fixed(2),
            "4300.00"
        );
        assert_eq!(out.fair_value_gain_loss_in_oci.to_fixed(2), "300.00");
        // the whole gain is OCI, so the profit-or-loss line stays zero
        assert_eq!(
            out.fair_value_gain_loss_in_statement_of_profit_or_loss,
            Money::zero()
        );
        // rent split 1000:3000 over a 4000 opening
        assert_eq!(
            out.fair_value_measurements.as_ref().unwrap()[0]
                .rental_income
                .to_fixed(2),
            "200.00"
        );
        assert_eq!(
            out.fair_value_measurements.as_ref().unwrap()[1]
                .rental_income
                .to_fixed(2),
            "600.00"
        );
        assert_eq!(
            out.fair_value_measurements.as_ref().unwrap()[1]
                .net_income
                .to_fixed(2),
            "375.00"
        );
        assert_eq!(
            out.direct_expenses_including_depreciation.to_fixed(2),
            "300.00"
        );
    }

    #[test]
    fn a_portfolio_with_no_opening_value_allocates_nothing_instead_of_raising() {
        let req = FairValueModelRequest {
            company_id: "C1".to_string(),
            reporting_date: "2026-10-31".to_string(),
            properties: vec![property("P1", "0", "800")],
            current_fair_values: BTreeMap::from([("P1".to_string(), m("1200"))]),
            rental_income: m("800"),
            direct_operating_expenses: m("300"),
        };
        let out = apply_fair_value_model(&req);
        assert_eq!(
            out.fair_value_measurements.as_ref().unwrap()[0].rental_income,
            Money::zero()
        );
        assert_eq!(
            out.fair_value_measurements.as_ref().unwrap()[0].net_income,
            Money::zero()
        );
        // the gain is still the full movement
        assert_eq!(out.fair_value_gain_loss_in_oci.to_fixed(2), "1200.00");
    }

    #[test]
    fn the_cost_model_depreciates_from_a_two_year_old_asset_and_sells_at_110_percent() {
        let req = CostModelRequest {
            company_id: "C1".to_string(),
            reporting_date: "2026-10-31".to_string(),
            properties: vec![property("P1", "1000", "100000")],
            depreciation_rate: 0.02,
        };
        let out = apply_cost_model(&req);
        let row = &out.cost_model_depreciations.as_ref().unwrap()[0];
        assert_eq!(row.depreciation_for_period.to_fixed(2), "2000.00");
        assert_eq!(row.accumulated_depreciation_opening.to_fixed(2), "4000.00");
        assert_eq!(row.accumulated_depreciation_closing.to_fixed(2), "6000.00");
        assert_eq!(row.disposal_proceeds.to_fixed(2), "110000.00");
        assert_eq!(row.carrying_amount.to_fixed(2), "94000.00");
        assert_eq!(row.gain_loss_on_disposal.to_fixed(2), "16000.00");
        assert_eq!(
            out.total_fair_value_or_carrying_amount.to_fixed(2),
            "94000.00"
        );
        assert_eq!(
            out.direct_expenses_including_depreciation.to_fixed(2),
            "2000.00"
        );
        assert!(out.fair_value_measurements.is_none());
    }

    #[test]
    fn a_zero_useful_life_depreciates_nothing_instead_of_raising() {
        let mut prop = property("P1", "1000", "100000");
        prop.useful_life_years = 0;
        let out = apply_cost_model(&CostModelRequest {
            company_id: "C1".to_string(),
            reporting_date: "2026-10-31".to_string(),
            properties: vec![prop],
            depreciation_rate: 0.0,
        });
        let row = &out.cost_model_depreciations.as_ref().unwrap()[0];
        assert_eq!(row.depreciation_for_period, Money::zero());
        assert_eq!(row.carrying_amount.to_fixed(2), "100000.00");
    }

    #[test]
    fn transfer_treatment_follows_the_upward_revaluation_rule() {
        let up = process_transfer(&TransferRequest {
            property_id: "P1".to_string(),
            reason_for_transfer: "now let".to_string(),
            carrying_amount: m("1000"),
            fair_value: m("1500"),
            model_from: "owner_occupied".to_string(),
            model_to: "investment".to_string(),
            transfer_date: "2026-10-31".to_string(),
        });
        assert_eq!(up.adjustment_to_property.unwrap().to_fixed(2), "500.00");
        assert_eq!(up.treatment, "recognized in OCI");

        let down = process_transfer(&TransferRequest {
            property_id: "P1".to_string(),
            reason_for_transfer: "now occupied".to_string(),
            carrying_amount: m("1000"),
            fair_value: m("900"),
            model_from: "owner_occupied".to_string(),
            model_to: "investment".to_string(),
            transfer_date: "2026-10-31".to_string(),
        });
        assert_eq!(down.treatment, "recognized in P&L");

        let out = process_transfer(&TransferRequest {
            property_id: "P1".to_string(),
            reason_for_transfer: "occupied".to_string(),
            carrying_amount: m("1000"),
            fair_value: m("900"),
            model_from: "investment".to_string(),
            model_to: "owner_occupied".to_string(),
            transfer_date: "2026-10-31".to_string(),
        });
        assert_eq!(out.fair_value_at_transfer, None);
        assert_eq!(
            out.treatment,
            "cost becomes deemed cost for subsequent measurement"
        );
    }

    #[test]
    fn disclosure_tiers_are_by_size_alone() {
        let req = DisclosureRequest {
            properties: vec![
                property("P1", "900000", "800000"),
                property("P2", "2000000", "1800000"),
                property("P3", "20000000", "19000000"),
            ],
            current_fair_values: BTreeMap::new(),
        };
        let out = prepare_fair_value_disclosure(&req);
        assert_eq!(out.level_1_count, 1);
        assert_eq!(out.level_2_count, 1);
        assert_eq!(out.level_3_count, 1);
        assert_eq!(out.level_1_fair_value.to_fixed(2), "900000.00");
        assert_eq!(out.valuation_technique, "Discounted cash flows");
        assert_eq!(out.significant_unobservable_inputs.len(), 3);
    }

    #[test]
    fn yields_charge_a_flat_thirty_percent_and_guard_a_zero_rate() {
        let out = calculate_property_yields(&YieldRequest {
            rental_income_annual: m("120000"),
            current_fair_value: m("2000000"),
            capitalization_rate: 6.0,
        });
        assert_eq!(out.gross_yield, 6.0);
        assert_eq!(out.net_yield, 4.2);
        assert_eq!(out.market_value_implied.to_fixed(2), "2000000.00");
        assert_eq!(out.capital_growth, 0.0);

        let none = calculate_property_yields(&YieldRequest {
            rental_income_annual: m("120000"),
            current_fair_value: m("2000000"),
            capitalization_rate: 0.0,
        });
        assert_eq!(none.market_value_implied, Money::zero());
        assert_eq!(none.capital_growth, 0.0);
    }

    #[test]
    fn amounts_past_double_precision_survive_the_ladder() {
        let req = FairValueModelRequest {
            company_id: "C1".to_string(),
            reporting_date: "2026-10-31".to_string(),
            properties: vec![property(
                "P1",
                "1234567890123456789010.55",
                "1234567890123456789000.00",
            )],
            current_fair_values: BTreeMap::from([(
                "P1".to_string(),
                m("1234567890123456789011.05"),
            )]),
            rental_income: m("1000.00"),
            direct_operating_expenses: m("0.45"),
        };
        let out = apply_fair_value_model(&req);
        let row = &out.fair_value_measurements.as_ref().unwrap()[0];
        assert_eq!(row.fair_value_gain_loss.to_fixed(2), "0.50");
        assert_eq!(row.net_income.to_fixed(2), "999.55");
        assert_eq!(
            out.total_fair_value_or_carrying_amount.to_fixed(2),
            "1234567890123456789011.05"
        );
    }
}
