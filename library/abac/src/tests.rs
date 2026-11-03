//! Behaviour pinned from policy-engine-service, plus what production needed.

use serde_json::{json, Value};

use crate::decision::EvaluationContext;
use crate::operator::{compare, Comparison, Operator};
use crate::rule::{Effect, Rule, RuleError};
use crate::store::AbacStore;

fn amount_rule(id: &str, name: &str, operator: &str, value: Value, effect: Effect) -> Rule {
    Rule::new(
        id, "org-1", "invoice", name, "amount", operator, value, effect,
    )
}

/// The service allowed anything no deny blocked, so parity tests run with the
/// permissive default; the stricter default has its own test below.
fn store() -> AbacStore {
    let mut store = AbacStore::new().with_default_effect(Effect::Allow);
    store
        .add_rule(amount_rule(
            "rule-large",
            "Large invoice needs a second pair of eyes",
            ">",
            json!(10000),
            Effect::RequireApproval,
        ))
        .unwrap();
    store
        .add_rule(
            Rule::new(
                "rule-currency",
                "org-1",
                "invoice",
                "Restricted currencies",
                "currency",
                "in",
                json!(["ZWL", "USD"]),
                Effect::Deny,
            )
            .with_message("Currency not permitted for this book"),
        )
        .unwrap();
    store
        .add_rule(
            Rule::new(
                "rule-petty",
                "org-1",
                "invoice",
                "Petty cash warning",
                "amount",
                "<",
                json!(50),
                Effect::Warn,
            )
            .with_message("Unusually small invoice"),
        )
        .unwrap();
    store
}

fn context(amount: f64) -> EvaluationContext {
    EvaluationContext::new("org-1", "invoice")
        .with_attribute("amount", json!(amount))
        .with_attribute("currency", json!("EUR"))
}

#[test]
fn the_original_operator_set_behaves_as_before() {
    let value = json!(100);
    assert_eq!(
        compare(Operator::Gt, &json!(150), &value),
        Comparison::Matched
    );
    assert_eq!(
        compare(Operator::Gt, &json!(50), &value),
        Comparison::DidNotMatch
    );
    assert_eq!(
        compare(Operator::Lt, &json!(50), &value),
        Comparison::Matched
    );
    assert_eq!(
        compare(Operator::Gte, &json!(100), &value),
        Comparison::Matched
    );
    assert_eq!(
        compare(Operator::Lte, &json!(100), &value),
        Comparison::Matched
    );
    assert_eq!(
        compare(Operator::Eq, &json!(100), &value),
        Comparison::Matched
    );
    assert_eq!(
        compare(Operator::Contains, &json!("a transaction"), &json!("trans")),
        Comparison::Matched
    );
    assert_eq!(
        compare(
            Operator::Contains,
            &json!(["fuel", "travel"]),
            &json!("vel")
        ),
        Comparison::Matched,
        "a list matches when any element does"
    );
}

#[test]
fn a_missing_attribute_never_triggers() {
    let mut store = store();
    store
        .add_rule(amount_rule(
            "rule-cat",
            "Food needs review",
            "contains",
            json!("food"),
            Effect::Deny,
        ))
        .unwrap();
    // the rule's field is `amount`, which is present; nothing else in the
    // context is consulted
    let decision = store.decide(&context(5.0));
    assert!(decision
        .evaluations
        .iter()
        .all(|e| e.rule_id != "rule-cat" || !e.triggered));

    let empty = EvaluationContext::new("org-1", "invoice");
    let decision = store.decide(&empty);
    assert_eq!(decision.triggered_count, 0);
    let outcome = decision
        .evaluations
        .iter()
        .find(|e| e.rule_id == "rule-large")
        .unwrap();
    assert!(!outcome.triggered);
    assert_eq!(outcome.reason, "missing_attribute");
}

#[test]
fn a_deny_blocks_and_the_decision_shape_matches_the_service() {
    let decision = store().decide(&context(20.0).with_attribute("currency", json!("ZWL")));
    assert!(decision.blocked);
    assert!(!decision.allowed);
    assert_eq!(
        decision.triggered_count, 2,
        "the deny and the petty-cash warning fired; the large-invoice rule did not"
    );
    assert_eq!(decision.effect, Effect::Deny);
    assert_eq!(
        decision.decided_by.as_deref(),
        Some("Restricted currencies")
    );

    let body = decision.to_json();
    assert_eq!(body["blocked"], json!(true));
    assert_eq!(body["allowed"], json!(false));
    assert_eq!(body["triggered_count"], json!(2));
    assert_eq!(body["effect"], json!("deny"));
    assert_eq!(body["evaluations"].as_array().unwrap().len(), 3);
}

#[test]
fn warnings_and_approvals_do_not_block() {
    let decision = store().decide(&context(50_000.0));
    assert!(!decision.blocked);
    assert!(decision.allowed);
    assert_eq!(decision.effect, Effect::RequireApproval);
    assert_eq!(decision.obligations, vec!["require_approval".to_string()]);

    let small = store().decide(&context(10.0));
    assert!(small.allowed);
    assert_eq!(small.effect, Effect::Warn);
    assert_eq!(small.obligations, vec!["warn".to_string()]);
    assert_eq!(
        small.decided_by.as_deref(),
        Some("Petty cash warning"),
        "a warning is still reported as the deciding rule"
    );
}

#[test]
fn with_nothing_triggered_the_default_applies() {
    let store = AbacStore::new();
    let decision = store.decide(&context(100.0));
    assert!(decision.blocked, "deny by default");
    assert_eq!(decision.effect, Effect::Deny);

    let permissive = AbacStore::new().with_default_effect(Effect::Allow);
    let decision = permissive.decide(&context(100.0));
    assert!(decision.allowed);
    assert_eq!(decision.effect, Effect::Allow);
}

#[test]
fn rules_are_scoped_to_organisation_resource_type_and_action() {
    let store = store();
    // another organisation sees nothing
    let elsewhere = EvaluationContext::new("org-2", "invoice").with_attribute("amount", json!(1));
    assert_eq!(store.decide(&elsewhere).evaluations.len(), 0);
    // another resource type sees nothing
    let journal = EvaluationContext::new("org-1", "journal").with_attribute("amount", json!(1));
    assert_eq!(store.decide(&journal).evaluations.len(), 0);

    let mut store = store;
    store
        .add_rule(
            Rule::new(
                "rule-action",
                "org-1",
                "invoice",
                "Approval only on approve",
                "amount",
                ">",
                json!(0),
                Effect::Deny,
            )
            .for_action("approve"),
        )
        .unwrap();
    let read = store.decide(&context(100.0));
    assert!(
        !read
            .evaluations
            .iter()
            .any(|e| e.rule_name == "Approval only on approve" && e.triggered),
        "action scope excludes a read"
    );
    let approve = store.decide(&context(100.0).with_action("approve"));
    assert!(approve
        .evaluations
        .iter()
        .any(|e| e.rule_name == "Approval only on approve" && e.triggered));
}

#[test]
fn a_disabled_rule_is_listed_and_explained_but_never_fires() {
    let mut store = AbacStore::new().with_default_effect(Effect::Allow);
    store
        .add_rule(
            Rule::new(
                "rule-off",
                "org-1",
                "invoice",
                "Off switch",
                "amount",
                ">",
                json!(0),
                Effect::Deny,
            )
            .disabled(),
        )
        .unwrap();
    let decision = store.decide(&context(100.0));
    assert!(!decision.blocked);
    assert_eq!(
        decision.evaluations.len(),
        0,
        "disabled rules are not listed as applicable"
    );

    assert!(store.rules.get("rule-off").is_some());
    assert!(store.set_enabled("rule-off", true).is_ok());
    assert!(store.decide(&context(100.0)).blocked);
}

#[test]
fn an_unknown_operator_is_refused_when_the_rule_is_written() {
    let mut store = AbacStore::new().with_default_effect(Effect::Allow);
    let error = store
        .add_rule(Rule::new(
            "org-1",
            "org-1",
            "invoice",
            "Typo",
            "amount",
            "=>",
            json!(10),
            Effect::Deny,
        ))
        .unwrap_err();
    assert_eq!(error, RuleError::UnknownOperator("=>".to_string()));
    assert_eq!(error.status_code(), 400);
    assert_eq!(error.message(), "Unsupported condition operator: =>");
    assert_eq!(store.rules.len(), 0);
}

#[test]
fn list_operators_are_validated_against_their_shape() {
    let mut store = AbacStore::new().with_default_effect(Effect::Allow);
    let bad_between = store
        .add_rule(Rule::new(
            "org-1",
            "org-1",
            "invoice",
            "Between with one bound",
            "amount",
            "between",
            json!([10]),
            Effect::Warn,
        ))
        .unwrap_err();
    assert_eq!(
        bad_between,
        RuleError::InvalidConditionValue("between".to_string())
    );

    store
        .add_rule(Rule::new(
            "org-1",
            "org-1",
            "invoice",
            "In a band",
            "amount",
            "between",
            json!([100, 10]),
            Effect::Warn,
        ))
        .unwrap();
    // bounds are normalised, so a reversed range still works
    assert!(store.decide(&context(50.0)).allowed);
}

#[test]
fn rule_names_are_unique_per_organisation_and_updates_keep_the_id() {
    let mut store = store();
    let duplicate = store
        .add_rule(amount_rule(
            "another",
            "petty cash warning",
            ">",
            json!(1),
            Effect::Warn,
        ))
        .unwrap_err();
    assert_eq!(duplicate.status_code(), 409);

    // the same name in another organisation is fine
    store
        .add_rule(Rule::new(
            "org-2",
            "org-2",
            "invoice",
            "Petty cash warning",
            "amount",
            ">",
            json!(1),
            Effect::Warn,
        ))
        .unwrap();

    let updated = store
        .update_rule(
            "rule-large",
            Rule::new(
                "ignored",
                "org-1",
                "invoice",
                "Large invoice needs approval",
                "amount",
                ">=",
                json!(5000),
                Effect::RequireApproval,
            ),
        )
        .unwrap();
    assert_eq!(updated.id, "rule-large");
    assert_eq!(updated.name, "Large invoice needs approval");
    assert!(store.update_rule("missing", updated.clone()).is_err());
    assert!(store.delete_rule("rule-large").is_ok());
    assert_eq!(
        store.delete_rule("rule-large").unwrap_err(),
        RuleError::NotFound
    );
}

#[test]
fn priorities_order_the_explanation_not_the_outcome() {
    let mut store = AbacStore::new().with_default_effect(Effect::Allow);
    store
        .add_rule(
            amount_rule("low", "Low priority allow", ">", json!(0), Effect::Allow).with_priority(1),
        )
        .unwrap();
    store
        .add_rule(
            amount_rule("high", "High priority deny", ">", json!(100), Effect::Deny)
                .with_priority(10),
        )
        .unwrap();
    let decision = store.decide(&context(500.0));
    assert!(decision.blocked, "deny wins even though allow also fired");
    assert_eq!(decision.evaluations[0].rule_id, "high");
    assert_eq!(decision.evaluations[1].rule_id, "low");
    assert_eq!(decision.triggered_count, 2);
}

#[test]
fn attributes_that_arrive_as_strings_still_order() {
    // a CSV or form post sends "15000", and that must still trip a 10000 rule
    let decision = store().decide(&context(0.0).with_attribute("amount", json!("15000")));
    assert!(decision
        .evaluations
        .iter()
        .any(|e| e.rule_id == "rule-large" && e.triggered));
}

#[test]
fn a_type_mismatch_is_reported_rather_than_silently_ignored() {
    let decision =
        store().decide(&context(0.0).with_attribute("amount", json!({"unexpected": "object"})));
    let outcome = decision
        .evaluations
        .iter()
        .find(|e| e.rule_id == "rule-large")
        .unwrap();
    assert!(!outcome.triggered);
    assert_eq!(outcome.reason, "type_mismatch");
    // the order operators do not throw: a decision is still produced
    assert!(decision.allowed);
    assert_eq!(decision.triggered_count, 0);
}

#[test]
fn exists_rules_catch_an_attribute_that_should_have_been_there() {
    let mut store = AbacStore::new().with_default_effect(Effect::Allow);
    store
        .add_rule(
            Rule::new(
                "rule-vat",
                "org-1",
                "invoice",
                "VAT number required",
                "vat_number",
                "not_exists",
                Value::Null,
                Effect::Deny,
            )
            .with_message("VAT number is required"),
        )
        .unwrap();
    assert!(store.decide(&context(100.0)).blocked);
    let present = store.decide(&context(100.0).with_attribute("vat_number", json!("VAT-123")));
    assert!(present.allowed);
}

#[test]
fn multi_valued_attributes_match_any_element() {
    let mut store = AbacStore::new().with_default_effect(Effect::Allow);
    store
        .add_rule(
            Rule::new(
                "rule-departments",
                "org-1",
                "invoice",
                "Restricted departments",
                "department",
                "in",
                json!(["treasury", "executive"]),
                Effect::RequireApproval,
            )
            .with_message("Departmental approval needed"),
        )
        .unwrap();
    let context = context(100.0).with_attribute("department", json!(["finance", "treasury"]));
    let decision = store.decide(&context);
    assert_eq!(decision.effect, Effect::RequireApproval);
    assert_eq!(decision.obligations, vec!["require_approval".to_string()]);

    let not_listed = store.decide(&context.with_attribute("department", json!(["finance"])));
    assert!(not_listed.allowed);
    assert_eq!(not_listed.obligations.len(), 0);
}

#[test]
fn in_and_not_in_are_mirrors() {
    // deny anything routed to a sanctioned bank
    let mut sanctioned = AbacStore::new().with_default_effect(Effect::Allow);
    sanctioned
        .add_rule(Rule::new(
            "rule-sanctions",
            "org-1",
            "payment",
            "Sanctioned routes",
            "route",
            "in",
            json!(["sanctioned-bank"]),
            Effect::Deny,
        ))
        .unwrap();
    assert!(
        sanctioned
            .decide(
                &EvaluationContext::new("org-1", "payment")
                    .with_attribute("route", json!("sanctioned-bank"))
            )
            .blocked
    );
    assert!(
        sanctioned
            .decide(
                &EvaluationContext::new("org-1", "payment")
                    .with_attribute("route", json!("normal-bank"))
            )
            .allowed
    );

    // the same intent written the other way: deny anything outside the corridor
    let mut corridor = AbacStore::new().with_default_effect(Effect::Allow);
    corridor
        .add_rule(Rule::new(
            "rule-corridor",
            "org-1",
            "payment",
            "Outside the permitted corridor",
            "route",
            "not_in",
            json!(["normal-bank", "reserve-bank"]),
            Effect::Deny,
        ))
        .unwrap();
    assert!(
        corridor
            .decide(
                &EvaluationContext::new("org-1", "payment")
                    .with_attribute("route", json!("sanctioned-bank"))
            )
            .blocked
    );
    assert!(
        corridor
            .decide(
                &EvaluationContext::new("org-1", "payment")
                    .with_attribute("route", json!("reserve-bank"))
            )
            .allowed
    );
    // A null route neither satisfies nor breaks an ordinary comparison, so a
    // corridor rule belongs beside a presence guard. That pairing closes the gap.
    assert!(
        !corridor
            .decide(
                &EvaluationContext::new("org-1", "payment")
                    .with_attribute("route", serde_json::Value::Null)
            )
            .blocked
    );
    corridor
        .add_rule(Rule::new(
            "rule-route-present",
            "org-1",
            "payment",
            "Route must be stated",
            "route",
            "not_exists",
            serde_json::Value::Null,
            Effect::Deny,
        ))
        .unwrap();
    assert!(
        corridor
            .decide(
                &EvaluationContext::new("org-1", "payment")
                    .with_attribute("route", serde_json::Value::Null)
            )
            .blocked
    );
    // an absent key reads the same as an explicit null for a presence rule
    assert!(
        corridor
            .decide(&EvaluationContext::new("org-1", "payment"))
            .blocked
    );
}

#[test]
fn every_rule_carries_a_reason_into_the_response() {
    let decision = store().decide(&context(20.0));
    let body = decision.to_json();
    let reasons: Vec<&str> = body["evaluations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|evaluation| evaluation["reason"].as_str().unwrap())
        .collect();
    assert!(reasons.contains(&"matched"));
    assert!(reasons.contains(&"no_match"));
}

#[test]
fn organisation_scoping_and_health() {
    let mut store = store();
    store
        .add_rule(Rule::new(
            "org-2",
            "org-2",
            "invoice",
            "Other org rule",
            "amount",
            ">",
            json!(1),
            Effect::Warn,
        ))
        .ok();
    let health = store.health();
    assert_eq!(health["status"], "healthy");
    assert_eq!(health["rules"], 4);
    assert_eq!(health["default_effect"], "allow");
    assert_eq!(store.rules.all("org-1").len(), 3);
    assert_eq!(store.rules.all("org-2").len(), 1);
}

#[test]
fn the_sequence_of_a_real_decision_is_auditable() {
    // large amount, restricted currency, in one request
    let decision = store().decide(&context(50_000.0).with_attribute("currency", json!("USD")));
    assert!(decision.blocked);
    assert_eq!(decision.obligations, vec!["require_approval".to_string()]);
    let names: Vec<&str> = decision
        .evaluations
        .iter()
        .filter(|e| e.triggered)
        .map(|e| e.rule_name.as_str())
        .collect();
    assert_eq!(names.len(), 2);
}
