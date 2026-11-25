//! What an automation step actually does in a Book.
//!
//! A rule is a description: steps, their order, and the action each one names. This crate is the hands. It answers the
//! actions that are the clock's own work - writing a month's depreciation off the register, closing a month - and it
//! refuses everything else by name, because a step nothing can do must not be recorded as done.
//!
//! Two rules shape every action here:
//!
//! 1. **A refusal is a sentence, never a silent skip.** A run that stops says which step and why, in words a person can
//!    act on: "No asset is registered and held, so there is nothing to depreciate for 2026-11."
//! 2. **Doing it twice is refused, not repeated.** Depreciation already charged for a month is not charged again, and a
//!    month already closed is not closed again; both are answered as already done. That is what makes it safe for the
//!    worker to resume a run it claimed before a crash: it finishes the work that is left and cannot double it.
#![forbid(unsafe_code)]

pub mod standard;

use automation::handler::{ActionRequest, StepHandler};
use common::dates::{month_before, written_at};
use serde_json::{json, Value};
use store::{BookScope, Graph, StoreError};

/// The actions this runner knows. A person choosing an action for a step is told these, rather than finding out when the
/// rule runs that nothing can do what they asked.
pub const ACTIONS: [&str; 2] = ["post_depreciation", "close_period"];

/// The hands a rule is given, bound to one Book and one instant.
pub struct EngineActions<'a, G: Graph + ?Sized> {
    graph: &'a G,
    scope: &'a BookScope,
    /// The instant the run is happening. An action that needs a period takes it from here when the step does not name
    /// one, which is what "automatic" means: the rule says what, the clock says when.
    now: f64,
}

impl<'a, G: Graph + ?Sized> EngineActions<'a, G> {
    pub fn new(graph: &'a G, scope: &'a BookScope, now: f64) -> Self {
        Self { graph, scope, now }
    }

    /// The month this run falls in, as the engine writes a period.
    fn month_now(&self) -> String {
        written_at(self.now).chars().take(7).collect()
    }

    /// The day this run falls on, as the engine writes a date.
    fn day_now(&self) -> String {
        written_at(self.now).chars().take(10).collect()
    }

    /// The period a step asks for: the month it names, the month that just ended, or the month the run falls in.
    ///
    /// "last_month" exists because that is what a month-end rule means: a rule that fires on the first of December is
    /// charging November, and saying so in the rule keeps the rule honest about which month it is about.
    fn period_of(&self, request: &ActionRequest<'_>) -> String {
        match request.setting("period").as_str() {
            "" => self.month_now(),
            "last_month" => month_before(self.now),
            named => named.to_string(),
        }
    }

    /// Write off one month's depreciation for every asset the Book still holds.
    ///
    /// One asset is not allowed to stop the others: a register of ten vans should not go uncharged because one of them
    /// was disposed of without being marked. Each refusal is counted, and the run is only stopped when something is
    /// wrong that a person has to fix, in which case the sentence names it.
    fn post_depreciation(&self, request: &ActionRequest<'_>) -> Result<Value, String> {
        let period = self.period_of(request);
        if period.len() != 7 || !period.contains('-') {
            return Err(format!(
                "A period reads as a month: write it like 2026-11, the year and then the month; got \"{period}\"."
            ));
        }

        let held = assets::fixed_assets_register::list_assets(
            self.graph,
            self.scope,
            None,
            Some("active"),
            None,
        )
        .map_err(|error| format!("The asset register could not be read: {error}"))?;
        let rows = held.as_array().cloned().unwrap_or_default();
        if rows.is_empty() {
            // nothing to charge is a state, not a fault: the Book holds no asset, so the month costs nothing and the run
            // says so instead of failing every month on a Book that has never bought anything
            return Ok(json!({
                "period": period,
                "charged": 0,
                "already charged": 0,
                "note": "No asset is registered and held, so nothing was written off.",
            }));
        }

        let mut charged: Vec<Value> = Vec::new();
        let mut already: Vec<String> = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        for asset in &rows {
            let id = asset
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let name = asset
                .get("asset_name")
                .and_then(Value::as_str)
                .unwrap_or(&id)
                .to_string();
            match assets::fixed_assets_register::depreciate(self.graph, self.scope, &id, &period) {
                Ok(entry) => charged.push(json!({
                    "asset": name,
                    "charge": entry.get("amount").cloned().unwrap_or(Value::Null),
                })),
                // the register refuses a second charge for the same month, which is how a resumed run stays honest
                Err(StoreError::Invalid(reason)) if reason.contains("already recorded") => {
                    already.push(name)
                }
                Err(other) => failed.push(format!("{name}: {other}")),
            }
        }

        if !failed.is_empty() {
            return Err(format!(
                "Depreciation for {period} was charged on {} asset(s) and refused on {}: {}",
                charged.len(),
                failed.len(),
                failed.join("; ")
            ));
        }

        Ok(json!({
            "period": period,
            "charged": charged.len(),
            "already charged": already.len(),
            "assets": charged,
        }))
    }

    /// Close a month.
    ///
    /// A month that is already closed is not an error: the run asked for a state, and the Book is in it. Anything else
    /// the periods module refuses is passed on as the reason the run stopped.
    fn close_period(&self, request: &ActionRequest<'_>) -> Result<Value, String> {
        let period = self.period_of(request);
        let closes_on = {
            let said = request.setting("closes_on");
            if said.is_empty() {
                self.day_now()
            } else {
                said
            }
        };
        let asked = json!({"period": period, "closes_on": closes_on, "closed_by": "automation"});
        match accounting::periods::close_period(self.graph, self.scope, &asked) {
            Ok(closed) => Ok(json!({
                "period": period,
                "closed": true,
                "closed_on": closes_on,
                "record": closed,
            })),
            Err(StoreError::Invalid(reason)) if reason.contains("already closed") => Ok(json!({
                "period": period,
                "closed": true,
                "already closed": true,
            })),
            Err(other) => Err(format!("{period} could not be closed: {other}")),
        }
    }
}

impl<G: Graph + ?Sized> StepHandler for EngineActions<'_, G> {
    fn act(&self, request: &ActionRequest<'_>) -> Result<Value, String> {
        match request.action {
            "post_depreciation" => self.post_depreciation(request),
            "close_period" => self.close_period(request),
            other => Err(format!(
                "Nothing here can do \"{other}\", so step {} was not taken. An action is one of {}.",
                request.step_id,
                ACTIONS.join(", ")
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    /// 2026-11-18T09:00:00Z
    fn november() -> f64 {
        1_794_992_400.0
    }

    fn asset(graph: &MemoryGraph, code: &str, cost: &str) -> Value {
        assets::fixed_assets_register::create_asset(
            graph,
            &scope(),
            &json!({
                "asset_code": code,
                "asset_name": format!("Van {code}"),
                "category": "vehicles",
                "acquisition_cost": cost,
                "acquisition_date": "2026-01-01",
                "useful_life_years": 10,
                "salvage_value": "0",
                "depreciation_method": "straight_line",
            }),
        )
        .expect("an asset")
    }

    fn step(action: &str, params: Value) -> Value {
        json!({"step_id": "s1", "step_name": action, "action": action, "params": params})
    }

    fn request<'a>(action: &'a str, params: &'a Value) -> ActionRequest<'a> {
        ActionRequest {
            run_id: "run-r1-1763456400",
            step_id: "s1",
            action,
            params,
        }
    }

    #[test]
    fn a_month_of_depreciation_is_written_off_every_asset_held() {
        let graph = MemoryGraph::new();
        asset(&graph, "V1", "120000");
        asset(&graph, "V2", "60000");
        let book = scope();
        let hands = EngineActions::new(&graph, &book, november());

        let params = step("post_depreciation", json!({}));
        let answered = hands
            .act(&request("post_depreciation", &params["params"]))
            .expect("the month is charged");
        assert_eq!(answered["period"], json!("2026-11"));
        assert_eq!(answered["charged"], json!(2));
        assert_eq!(answered["already charged"], json!(0));

        // and the register now says so, which is the only proof that matters
        let charged = assets::fixed_assets_register::list_assets(
            &graph,
            &scope(),
            None,
            Some("active"),
            None,
        )
        .expect("assets");
        let written_down: i64 = charged
            .as_array()
            .expect("assets")
            .iter()
            .map(|row| {
                row.get("accumulated_depreciation")
                    .and_then(Value::as_str)
                    .unwrap_or("0")
                    .parse::<f64>()
                    .map(|value| if value > 0.0 { 1 } else { 0 })
                    .unwrap_or(0)
            })
            .sum();
        assert_eq!(written_down, 2);
    }

    #[test]
    fn a_month_already_charged_is_not_charged_again() {
        let graph = MemoryGraph::new();
        asset(&graph, "V1", "120000");
        let book = scope();
        let hands = EngineActions::new(&graph, &book, november());
        let params = step("post_depreciation", json!({}));
        hands
            .act(&request("post_depreciation", &params["params"]))
            .expect("first charge");

        // a resumed run walks the same step again; the register refuses the second charge and the run says so plainly
        let again = hands
            .act(&request("post_depreciation", &params["params"]))
            .expect("a repeat is not a failure");
        assert_eq!(again["charged"], json!(0));
        assert_eq!(again["already charged"], json!(1));
        let held = assets::fixed_assets_register::list_assets(&graph, &scope(), None, None, None)
            .expect("assets");
        let row = held.as_array().expect("assets")[0].clone();
        let charged_entries = assets::fixed_assets_register::list_entries(
            &graph,
            &scope(),
            row.get("id").and_then(Value::as_str).unwrap_or_default(),
        )
        .expect("entries");
        assert_eq!(charged_entries.as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn a_register_with_nothing_in_it_is_answered_rather_than_failed() {
        // a Book that has never bought an asset should not fail its month-end rule every month: nothing to charge is a
        // state, and the run says which state it found
        let graph = MemoryGraph::new();
        let book = scope();
        let hands = EngineActions::new(&graph, &book, november());
        let params = step("post_depreciation", json!({}));
        let answered = hands
            .act(&request("post_depreciation", &params["params"]))
            .expect("an empty register is not a fault");
        assert_eq!(answered["charged"], json!(0));
        assert_eq!(answered["period"], json!("2026-11"));
        assert!(answered["note"]
            .as_str()
            .unwrap_or_default()
            .contains("No asset is registered"));
    }

    #[test]
    fn a_month_end_rule_charges_the_month_that_just_ended() {
        // a rule that fires on the first of December is charging November, and the rule says so in one word
        let graph = MemoryGraph::new();
        let book = scope();
        let first_of_december = common::dates::first_of_next_month(november());
        let hands = EngineActions::new(&graph, &book, first_of_december);
        let params = step("post_depreciation", json!({"period": "last_month"}));
        let answered = hands
            .act(&request("post_depreciation", &params["params"]))
            .expect("the month before is charged");
        assert_eq!(answered["period"], json!("2026-11"));
    }

    #[test]
    fn a_period_that_is_not_a_month_is_refused() {
        let graph = MemoryGraph::new();
        asset(&graph, "V1", "120000");
        let book = scope();
        let hands = EngineActions::new(&graph, &book, november());
        let params = step("post_depreciation", json!({"period": "2026"}));
        let refused = hands
            .act(&request("post_depreciation", &params["params"]))
            .expect_err("a year is not a month");
        assert!(refused.contains("write it like 2026-11"), "{refused}");
    }

    #[test]
    fn a_month_is_closed_once_and_closing_it_again_is_answered_not_repeated() {
        let graph = MemoryGraph::new();
        let book = scope();
        let hands = EngineActions::new(&graph, &book, november());
        let params = step(
            "close_period",
            json!({"period": "2026-10", "closes_on": "2026-11-18"}),
        );
        let closed = hands
            .act(&request("close_period", &params["params"]))
            .expect("the month closes");
        assert_eq!(closed["closed"], json!(true));
        assert_eq!(closed["already closed"], Value::Null);

        let again = hands
            .act(&request("close_period", &params["params"]))
            .expect("a closed month is not a failure");
        assert_eq!(again["already closed"], json!(true));

        let months = accounting::periods::list_periods(&graph, &scope()).expect("periods");
        assert_eq!(months["closed_months"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn a_month_that_has_not_started_is_refused_by_the_periods_module_and_says_why() {
        let graph = MemoryGraph::new();
        let book = scope();
        let hands = EngineActions::new(&graph, &book, november());
        let params = step(
            "close_period",
            json!({"period": "2027-03", "closes_on": "2026-11-18"}),
        );
        let refused = hands
            .act(&request("close_period", &params["params"]))
            .expect_err("a future month cannot close");
        assert!(refused.contains("2027-03"), "{refused}");
        assert!(refused.contains("could not be closed"), "{refused}");
    }

    #[test]
    fn an_action_nothing_can_do_is_refused_naming_the_ones_that_work() {
        let graph = MemoryGraph::new();
        let book = scope();
        let hands = EngineActions::new(&graph, &book, november());
        let params = step("post_payroll", json!({}));
        let refused = hands
            .act(&request("post_payroll", &params["params"]))
            .expect_err("nothing here posts payroll");
        assert!(
            refused.contains("Nothing here can do \"post_payroll\""),
            "{refused}"
        );
        assert!(refused.contains("post_depreciation"), "{refused}");
        assert!(refused.contains("close_period"), "{refused}");
    }
}
