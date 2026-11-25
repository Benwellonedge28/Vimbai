//! The rules a new Book starts with.
//!
//! Autonomy is not a switch a person has to find: a Book that has just been opened should already be charging its
//! depreciation every month, because the alternative is an owner who is told the system is automatic and then discovers
//! that nothing happens until they build a rule. So a Book is given the clock's own work as rules, once, when it starts.
//!
//! Two rules are shipped, and they are not the same kind of thing:
//!
//! 1. **Charge depreciation every month** is on from the start. It cannot do harm: on a Book that holds no asset it
//!    answers that nothing was written off, and on a Book that holds assets it writes off the month that just ended,
//!    which is what a person would otherwise have to remember to do twelve times a year.
//! 2. **Close the month** is shipped switched *off*. Whether a clean month closes itself is a decision about the owner's
//!    own books, not a technical default, and it has not been decided. A rule that says what it would do, and is off
//!    until someone says otherwise, is the honest way to ship a decision nobody has made yet.
//!
//! Installing is safe to repeat: a rule whose name the Book already has is left alone, so a second call adds nothing and
//! never overwrites a rule a person has since edited.
#![forbid(unsafe_code)]

use common::dates::first_of_next_month;
use serde_json::{json, Value};
use store::read::text;
use store::{BookScope, Graph, StoreError};

use automation::automations::{automations, create_automation_rule};

/// The rules a Book is given when it starts.
pub fn standard_rules(now: f64) -> Vec<Value> {
    let due = first_of_next_month(now);
    vec![
        json!({
            "name": "Charge depreciation every month",
            "description": "On the first day of each month, writes off one month's depreciation for every asset the \
        Book still holds, for the month that just ended.",
            "trigger": "scheduled",
            "enabled": true,
            "condition": { "schedule": { "due_at": due, "every": "month" } },
            "steps": [{
                "step_id": "depreciate",
                "step_name": "Charge depreciation",
                "action": "post_depreciation",
                "params": { "period": "last_month" },
            }],
        }),
        json!({
            "name": "Close the month",
            "description": "Closes the month that just ended, once it is over. Switched off, because whether a clean \
        month should close itself is the owner's decision.",
            "trigger": "scheduled",
            "enabled": false,
            "condition": { "schedule": { "due_at": due, "every": "month" } },
            "steps": [{
                "step_id": "close",
                "step_name": "Close the month",
                "action": "close_period",
                "params": { "period": "last_month" },
            }],
        }),
    ]
}

/// Give a Book the rules it starts with, and answer what was added and what was already there.
pub fn install_standard_rules(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    now: f64,
) -> Result<Value, StoreError> {
    let held = automations(graph, scope, &json!({}))?;
    let existing: Vec<String> = held
        .as_array()
        .map(|rows| rows.iter().map(|row| text(row, "name")).collect())
        .unwrap_or_default();

    let mut added: Vec<String> = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    for rule in standard_rules(now) {
        let name = text(&rule, "name");
        if existing.contains(&name) {
            kept.push(name);
            continue;
        }
        create_automation_rule(graph, scope, &rule)?;
        added.push(name);
    }

    Ok(json!({ "added": added, "already there": kept }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use store::fake::MemoryGraph;

    fn talked() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    /// 2026-11-18T09:00:00Z
    fn november() -> f64 {
        1_794_992_400.0
    }

    #[test]
    fn a_new_book_is_given_the_clock_work_it_would_otherwise_have_to_build() {
        let graph = MemoryGraph::new();
        let answered = install_standard_rules(&graph, &talked(), november()).expect("installed");
        assert_eq!(answered["added"].as_array().map(Vec::len), Some(2));

        let rules = automations(&graph, &talked(), &json!({})).expect("rules");
        let rows = rules.as_array().expect("rules");
        let depreciation = rows
            .iter()
            .find(|row| text(row, "name") == "Charge depreciation every month")
            .expect("the depreciation rule");
        // it is on, it fires at the start of the next month, and it repeats by the calendar rather than by thirty days
        assert_eq!(depreciation["enabled"], json!(true));
        assert_eq!(
            depreciation["condition"]["schedule"]["every"],
            json!("month")
        );
        assert_eq!(
            depreciation["condition"]["schedule"]["due_at"].as_f64(),
            Some(common::dates::instant_of("2026-12-01").expect("a date"))
        );
        assert_eq!(
            depreciation["steps"][0]["params"]["period"],
            json!("last_month")
        );

        // closing a month is a decision about the owner's books, so the rule ships switched off
        let closing = rows
            .iter()
            .find(|row| text(row, "name") == "Close the month")
            .expect("the closing rule");
        assert_eq!(closing["enabled"], json!(false));
    }

    #[test]
    fn installing_twice_adds_nothing_and_never_overwrites_what_a_person_changed() {
        let graph = MemoryGraph::new();
        install_standard_rules(&graph, &talked(), november()).expect("installed");
        let rules = automations(&graph, &talked(), &json!({})).expect("rules");
        let rows = rules.as_array().cloned().unwrap_or_default();
        let depreciation = rows
            .iter()
            .find(|row| text(row, "name") == "Charge depreciation every month")
            .expect("the rule")
            .clone();

        // a person switches the rule off; a second install must not switch it back on
        automation::automations::toggle_automation(&graph, &talked(), &text(&depreciation, "id"))
            .expect("a toggle");

        let again = install_standard_rules(&graph, &talked(), november()).expect("installed");
        assert_eq!(again["added"].as_array().map(Vec::len), Some(0));
        assert_eq!(again["already there"].as_array().map(Vec::len), Some(2));

        let after = automations(&graph, &talked(), &json!({})).expect("rules");
        let rows = after.as_array().cloned().unwrap_or_default();
        assert_eq!(rows.len(), 2, "two rules, not four");
        let still_off = rows
            .iter()
            .find(|row| text(row, "name") == "Charge depreciation every month")
            .expect("the rule");
        assert_eq!(still_off["enabled"], json!(false));
    }

    #[test]
    fn a_second_book_is_given_its_own_rules_and_sees_none_of_the_first_books() {
        let graph = MemoryGraph::new();
        install_standard_rules(&graph, &talked(), november()).expect("installed");
        let elsewhere = BookScope::new("user-2", "book-2");
        assert_eq!(
            automations(&graph, &elsewhere, &json!({}))
                .expect("rules")
                .as_array()
                .map(Vec::len),
            Some(0)
        );
        install_standard_rules(&graph, &elsewhere, november()).expect("installed");
        assert_eq!(
            store::Records::new(&graph, automation::automations::RULE)
                .list(&talked(), 100)
                .expect("rules")
                .len(),
            2
        );
    }
}
