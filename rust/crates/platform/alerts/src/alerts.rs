#![forbid(unsafe_code)]
//! alerts-service: the rules a Book watches and the alerts those rules raise.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a rule's name is between three and a hundred letters long, its category, severity and action are the ones the
//!   service names, and its cooldown is a whole number of seconds that may be nothing. A rule arrives enabled with a
//!   trigger count of nought;
//! * replacing a rule keeps the bookkeeping - how often it has fired and when it last did - so a rule that is edited
//!   does not look like a rule that was never triggered;
//! * a condition is a threshold (a field, an operator and a value, all three required, the operator one of the six
//!   named), a pattern (a velocity of more than a threshold inside an hour) or a deadline (within a number of days of
//!   now, seven when nothing is said). Anything else, or a field the data does not carry, or a value that is not a
//!   number, is NOT a match - the shipped engine answered false rather than raising;
//! * evaluating watches only ENABLED rules, and holds a rule back while its cooldown has not run out. A rule that fires
//!   raises an alert titled after the rule, counts itself fired, and remembers when;
//! * triggering a rule by name fires whatever the rule's cooldown and whether or not it is enabled, because the shipped
//!   endpoint was for other services to shout through rather than a watch of its own;
//! * an alert is raised active, and is acknowledged, resolved or dismissed with the moment it happened, the status it
//!   was in being of no consequence - the shipped endpoints never checked what it had been;
//! * every alert answer is the caller's own; another Book's rule or alert is simply missing.
//!
//! Two differences forced by where this runs: the shipped service broadcast each alert over a live socket, and there
//! are no live sockets here, so the count of connected listeners a statistics question reports is nought. And "now" is
//! taken from the request rather than from a clock, so a cooldown or a deadline can be reckoned over and checked.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use vimbai_common::dates::days_between;
use vimbai_common::Money;
use vimbai_store::read::{decimal, field, flag, optional, or_default, refused, text, whole};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A watched rule.
pub const RULE: RecordKind = RecordKind::new("AlertRule", "OWNS_ALERT_RULE");

/// An alert a rule raised.
pub const ALERT: RecordKind = RecordKind::new("AlertRecord", "OWNS_ALERT");

/// The kinds an alert may be.
pub const CATEGORIES: [&str; 6] = [
    "fraud",
    "compliance",
    "financial",
    "security",
    "system",
    "workflow",
];

/// How loud an alert is.
pub const SEVERITIES: [&str; 5] = ["critical", "high", "medium", "low", "info"];

/// Where an alert stands.
pub const STATUSES: [&str; 4] = ["active", "acknowledged", "resolved", "dismissed"];

/// What a rule does when it fires.
pub const ACTIONS: [&str; 4] = ["notify", "email", "webhook", "auto_resolve"];

/// The comparisons a threshold condition may make.
pub const OPERATORS: [&str; 6] = ["gt", "gte", "lt", "lte", "eq", "neq"];

fn one_of(value: &str, allowed: &[&str], what: &str) -> Result<(), StoreError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(refused(&format!(
            "{what} must be one of {}",
            allowed.join(", ")
        )))
    }
}

fn rule_answer(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "name": field(record, "name"),
        "description": optional(record, "description"),
        "category": field(record, "category"),
        "severity": field(record, "severity"),
        "condition": record.get("condition").cloned().unwrap_or(Value::Null),
        "action": or_default(record, "action", "notify"),
        "action_config": record.get("action_config").cloned().unwrap_or(Value::Null),
        "enabled": flag(record, "enabled"),
        "cooldown_seconds": whole(record, "cooldown_seconds"),
        "trigger_count": whole(record, "trigger_count"),
        "last_triggered": optional(record, "last_triggered"),
        "created_by": field(record, "created_by"),
        "created_at": field(record, "created_at"),
        "updated_at": field(record, "updated_at"),
    })
}

fn alert_answer(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "rule_id": field(record, "rule_id"),
        "title": field(record, "title"),
        "message": field(record, "message"),
        "severity": field(record, "severity"),
        "category": field(record, "category"),
        "source": field(record, "source"),
        "metadata": record.get("metadata").cloned().unwrap_or(Value::Null),
        "status": or_default(record, "status", "active"),
        "created_at": field(record, "created_at"),
        "acknowledged_at": optional(record, "acknowledged_at"),
        "resolved_at": optional(record, "resolved_at"),
    })
}

/// The fields a rule is made of, checked.
fn rule_fields(payload: &Value) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.chars().count() < 3 || name.chars().count() > 100 {
        return Err(refused("name must be between 3 and 100 characters"));
    }
    let category = text(payload, "category");
    one_of(&category, &CATEGORIES, "category")?;
    let severity = text(payload, "severity");
    one_of(&severity, &SEVERITIES, "severity")?;
    let action = or_default(payload, "action", "notify");
    one_of(&action, &ACTIONS, "action")?;
    let cooldown = whole(payload, "cooldown_seconds");
    // a cooldown is a whole number of seconds, and anything else is refused rather than read as nought
    match payload.get("cooldown_seconds") {
        None | Some(Value::Null) => {}
        Some(Value::Number(_)) => {}
        Some(_) => return Err(refused("cooldown_seconds must be a number")),
    }
    Ok(json!({
        "name": name,
        "description": optional(payload, "description"),
        "category": category,
        "severity": severity,
        "condition": payload.get("condition").cloned().unwrap_or_else(|| json!({})),
        "action": action,
        "action_config": payload.get("action_config").cloned().unwrap_or(Value::Null),
        "enabled": match payload.get("enabled") {
            None | Some(Value::Null) => true,
            Some(_) => flag(payload, "enabled"),
        },
        "cooldown_seconds": cooldown,
    }))
}

/// `POST /rules`: set a rule to watch with.
pub fn set_alert_rule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let mut fields = rule_fields(payload)?;
    fields["id"] = json!(vimbai_common::ids::service_id());
    fields["trigger_count"] = json!(0);
    fields["last_triggered"] = Value::Null;
    fields["created_by"] = json!(text(payload, "created_by"));
    fields["created_at"] = json!(text(payload, "created_at"));
    fields["updated_at"] = json!(text(payload, "created_at"));
    let created = Records::new(graph, RULE).create(scope, &fields)?;
    Ok(rule_answer(&created))
}

/// `GET /rules`: the caller's rules, narrowed to the ones switched on when asked.
pub fn alert_rules_held(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let enabled_only = flag(payload, "enabled_only");
    let held = Records::new(graph, RULE).list(scope, 1000)?;
    let mine: Vec<Value> = held
        .iter()
        .filter(|rule| !enabled_only || flag(rule, "enabled"))
        .map(rule_answer)
        .collect();
    Ok(Value::Array(mine))
}

/// `GET /rules/{id}`.
pub fn rule_held(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, RULE).get(scope, rule_id)?;
    Ok(rule_answer(&held))
}

/// `PUT /rules/{id}`: a whole replacement that keeps the rule's bookkeeping.
pub fn replace_rule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, RULE);
    let existing = records.get(scope, rule_id)?;
    let mut fields = rule_fields(payload)?;
    // how often it has fired and when it last did are not the caller's to rewrite
    fields["trigger_count"] = json!(whole(&existing, "trigger_count"));
    fields["last_triggered"] = json!(optional(&existing, "last_triggered"));
    fields["created_by"] = json!(field(&existing, "created_by"));
    fields["created_at"] = json!(field(&existing, "created_at"));
    fields["updated_at"] = json!(text(payload, "updated_at"));
    let replaced = records.update(scope, rule_id, &fields)?;
    Ok(rule_answer(&replaced))
}

/// `DELETE /rules/{id}`.
pub fn remove_alert_rule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, RULE).delete(scope, rule_id)?;
    Ok(json!({ "deleted": true, "rule_id": rule_id }))
}

/// `POST /alerts`: raise an alert directly.
pub fn create_alert(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let severity = text(payload, "severity");
    one_of(&severity, &SEVERITIES, "severity")?;
    let category = text(payload, "category");
    one_of(&category, &CATEGORIES, "category")?;
    let fields = json!({
        "id": vimbai_common::ids::service_id(),
        "rule_id": text(payload, "rule_id"),
        "title": text(payload, "title"),
        "message": text(payload, "message"),
        "severity": severity,
        "category": category,
        "source": text(payload, "source"),
        "metadata": payload.get("metadata").cloned().unwrap_or(Value::Null),
        "status": "active",
        "created_at": text(payload, "created_at"),
    });
    let created = Records::new(graph, ALERT).create(scope, &fields)?;
    Ok(alert_answer(&created))
}

/// `GET /alerts`: the caller's alerts, newest first, narrowed by status, category and severity.
pub fn list_alerts(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let status = text(payload, "status");
    let category = text(payload, "category");
    let severity = text(payload, "severity");
    let limit = {
        let asked = whole(payload, "limit") as usize;
        if asked == 0 {
            100
        } else {
            asked.min(1000)
        }
    };
    let mut held: Vec<Value> = Records::new(graph, ALERT)
        .list(scope, 1000)?
        .into_iter()
        .filter(|alert| status.is_empty() || field(alert, "status") == status)
        .filter(|alert| category.is_empty() || field(alert, "category") == category)
        .filter(|alert| severity.is_empty() || field(alert, "severity") == severity)
        .collect();
    held.sort_by(|left, right| field(right, "created_at").cmp(field(left, "created_at")));
    held.truncate(limit);
    Ok(Value::Array(held.iter().map(alert_answer).collect()))
}

/// `GET /alerts/{id}`.
pub fn alert_held(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    alert_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, ALERT).get(scope, alert_id)?;
    Ok(alert_answer(&held))
}

/// The three ways an alert stops demanding attention. None of them checks the status it was in, as shipped.
fn settle(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    alert_id: &str,
    payload: &Value,
    status: &str,
    stamp_key: &str,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, ALERT);
    records.get(scope, alert_id)?;
    let mut patch = json!({ "status": status });
    patch[stamp_key] = json!(text(payload, stamp_key));
    let changed = records.update(scope, alert_id, &patch)?;
    Ok(alert_answer(&changed))
}

/// `PUT /alerts/{id}/acknowledge`.
pub fn acknowledge_alert(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    alert_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    settle(
        graph,
        scope,
        alert_id,
        payload,
        "acknowledged",
        "acknowledged_at",
    )
}

/// `PUT /alerts/{id}/resolve`.
pub fn resolve_alert(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    alert_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    settle(graph, scope, alert_id, payload, "resolved", "resolved_at")
}

/// `PUT /alerts/{id}/dismiss`.
pub fn dismiss_alert(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    alert_id: &str,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, ALERT);
    records.get(scope, alert_id)?;
    let changed = records.update(scope, alert_id, &json!({ "status": "dismissed" }))?;
    Ok(alert_answer(&changed))
}

/// Whether a rule's condition is met by the data offered.
fn matches(condition: &Value, data: &Value) -> bool {
    match text(condition, "type").as_str() {
        "threshold" => {
            let field_name = text(condition, "field");
            let operator = text(condition, "operator");
            let threshold = condition.get("value").cloned().unwrap_or(Value::Null);
            // all three are required, and an operator the engine does not know is not a match
            if field_name.is_empty() || operator.is_empty() || threshold.is_null() {
                return false;
            }
            if !OPERATORS.contains(&operator.as_str()) {
                return false;
            }
            let offered = match data.get(&field_name) {
                None | Some(Value::Null) => return false,
                Some(held) => held.clone(),
            };
            // a value that is not a number is not a match, as the shipped engine's conversion was not
            let (offered, threshold) = match (
                Money::parse(&decimal(&json!({"v": offered}), "v", "")),
                Money::parse(&decimal(&json!({"v": threshold}), "v", "")),
            ) {
                (Ok(offered), Ok(threshold)) => (offered, threshold),
                _ => return false,
            };
            let order = offered.as_decimal().cmp(threshold.as_decimal());
            match operator.as_str() {
                "gt" => order.is_gt(),
                "gte" => !order.is_lt(),
                "lt" => order.is_lt(),
                "lte" => !order.is_gt(),
                "eq" => order.is_eq(),
                "neq" => !order.is_eq(),
                _ => false,
            }
        }
        "pattern" => {
            // a velocity: more movements than the threshold inside an hour, and no longer a window
            if text(condition, "pattern_type") != "velocity" {
                return false;
            }
            let threshold = whole(condition, "threshold");
            let threshold = if threshold == 0 { 10 } else { threshold };
            let count = whole(data, "transaction_count");
            let window = {
                let asked = whole(data, "time_window_minutes");
                if asked == 0 {
                    60
                } else {
                    asked
                }
            };
            count > threshold && window <= 60
        }
        "deadline" => {
            let days_before = {
                let asked = whole(condition, "days_before");
                if asked == 0 {
                    7
                } else {
                    asked
                }
            };
            let deadline = text(data, "deadline");
            if deadline.is_empty() {
                return false;
            }
            let now = text(data, "now");
            // a deadline that is now or within the window is a match; one already gone is not
            match days_between(&deadline, &now) {
                Some(days) => (0..=i64::from(days_before)).contains(&days),
                None => false,
            }
        }
        _ => false,
    }
}

/// `POST /evaluate`: watch the data against every enabled rule whose cooldown has run out.
pub fn evaluate_data(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let data = payload.get("data").cloned().unwrap_or_else(|| json!({}));
    let now = text(payload, "now");
    let rules = Records::new(graph, RULE).list(scope, 1000)?;
    let mut raised = Vec::new();
    for rule in &rules {
        if !flag(rule, "enabled") {
            continue;
        }
        // a rule that fired within its cooldown is held back
        let last = optional(rule, "last_triggered").unwrap_or_default();
        if !last.is_empty() {
            let cooldown = i64::from(whole(rule, "cooldown_seconds"));
            if let Some(elapsed) = seconds_between(&now, &last) {
                if elapsed < cooldown {
                    continue;
                }
            }
        }
        let condition = rule.get("condition").cloned().unwrap_or_else(|| json!({}));
        if !matches(&condition, &data) {
            continue;
        }
        let alert = Records::new(graph, ALERT).create(
            scope,
            &json!({
                "id": vimbai_common::ids::service_id(),
                "rule_id": field(rule, "id"),
                "title": format!("{}: Threshold exceeded", field(rule, "name")),
                "message": format!("Alert triggered for data: {data}"),
                "severity": field(rule, "severity"),
                "category": field(rule, "category"),
                "source": source_of(&data, "system"),
                "metadata": {"data": data.clone(), "rule_name": field(rule, "name")},
                "status": "active",
                "created_at": now.clone(),
            }),
        )?;
        Records::new(graph, RULE).update(
            scope,
            field(rule, "id"),
            &json!({
                "trigger_count": whole(rule, "trigger_count") + 1,
                "last_triggered": now.clone(),
            }),
        )?;
        raised.push(alert_answer(&alert));
    }
    Ok(json!({ "triggered_count": raised.len(), "alerts": raised }))
}

/// `POST /trigger/{rule_id}`: shout through a rule, whatever its cooldown and whether or not it is on.
pub fn trigger_rule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let rule = Records::new(graph, RULE).get(scope, rule_id)?;
    let told = text(payload, "message");
    let message = if told.is_empty() {
        format!("Alert triggered by rule: {}", field(&rule, "name"))
    } else {
        told
    };
    let source = source_of(payload, "internal");
    let alert = Records::new(graph, ALERT).create(
        scope,
        &json!({
            "id": vimbai_common::ids::service_id(),
            "rule_id": rule_id,
            "title": field(&rule, "name"),
            "message": message,
            "severity": field(&rule, "severity"),
            "category": field(&rule, "category"),
            "source": source,
            "metadata": payload.clone(),
            "status": "active",
            "created_at": text(payload, "now"),
        }),
    )?;
    Ok(alert_answer(&alert))
}

/// Who raised the alert: what the payload says, or the fallback the shipped service used.
fn source_of(payload: &Value, fallback: &str) -> String {
    let told = text(payload, "source");
    if told.is_empty() {
        fallback.to_string()
    } else {
        told
    }
}

/// `GET /stats`: what the caller's alerts look like, by status, severity and category.
pub fn alerts_counted(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let alerts = Records::new(graph, ALERT).list(scope, 1000)?;
    let rules = Records::new(graph, RULE).list(scope, 1000)?;
    let mut by_status: BTreeMap<String, i64> = BTreeMap::new();
    let mut by_severity: BTreeMap<String, i64> = BTreeMap::new();
    let mut by_category: BTreeMap<String, i64> = BTreeMap::new();
    for alert in &alerts {
        *by_status
            .entry(or_default(alert, "status", "active"))
            .or_insert(0) += 1;
        *by_severity
            .entry(or_default(alert, "severity", "info"))
            .or_insert(0) += 1;
        *by_category
            .entry(or_default(alert, "category", "system"))
            .or_insert(0) += 1;
    }
    Ok(json!({
        "total_alerts": alerts.len(),
        "by_status": by_status,
        "by_severity": by_severity,
        "by_category": by_category,
        // there are no live sockets here, so nobody is listening
        "active_connections": 0,
        "total_rules": rules.len(),
        "enabled_rules": rules.iter().filter(|rule| flag(rule, "enabled")).count(),
    }))
}

/// The seconds between two moments, as whole seconds, when both are moments this crate can read.
fn seconds_between(later: &str, earlier: &str) -> Option<i64> {
    let parse = |stamp: &str| -> Option<i64> {
        let trimmed = stamp.trim_end_matches('Z');
        let (date, clock) = trimmed.split_once('T')?;
        let day = vimbai_common::dates::day_of(date)?;
        let mut parts = clock.split(':');
        let hour: i64 = parts.next()?.parse().ok()?;
        let minute: i64 = parts.next()?.parse().ok()?;
        let second = parts.next().unwrap_or("0");
        let second: i64 = second.split('.').next()?.parse().ok()?;
        Some(day * 86_400 + hour * 3_600 + minute * 60 + second)
    };
    Some(parse(later)? - parse(earlier)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn a_rule(graph: &MemoryGraph, condition: Value, cooldown: i64) -> Value {
        set_alert_rule(
            graph,
            &scope(),
            &json!({
                "name": "Large payment",
                "category": "fraud",
                "severity": "critical",
                "condition": condition,
                "cooldown_seconds": cooldown,
                "created_at": "2026-01-01T09:00:00Z",
            }),
        )
        .expect("a rule")
    }

    #[test]
    fn a_rule_is_checked_before_it_is_watched() {
        let graph = MemoryGraph::new();
        assert!(set_alert_rule(
            &graph,
            &scope(),
            &json!({"name": "no", "category": "fraud", "severity": "critical"})
        )
        .is_err());
        assert!(set_alert_rule(
            &graph,
            &scope(),
            &json!({"name": "Big one", "category": "weather", "severity": "critical"})
        )
        .is_err());
        let made = set_alert_rule(
            &graph,
            &scope(),
            &json!({"name": "Big one", "category": "fraud", "severity": "critical",
                    "created_at": "2026-01-01T09:00:00Z"}),
        )
        .expect("a rule");
        assert_eq!(made["enabled"], json!(true));
        assert_eq!(made["cooldown_seconds"], json!(0));
        assert_eq!(made["trigger_count"], json!(0));
        assert_eq!(made["action"], json!("notify"));
    }

    #[test]
    fn a_threshold_fires_once_and_then_waits_out_its_cooldown() {
        let graph = MemoryGraph::new();
        a_rule(
            &graph,
            json!({"type": "threshold", "field": "amount", "operator": "gt", "value": 10000}),
            3600,
        );
        let fired = evaluate_data(
            &graph,
            &scope(),
            &json!({"data": {"amount": 12500, "source": "ledger"}, "now": "2026-01-02T09:00:00Z"}),
        )
        .expect("a watch");
        assert_eq!(fired["triggered_count"], json!(1));
        assert_eq!(
            fired["alerts"][0]["title"],
            json!("Large payment: Threshold exceeded")
        );
        assert_eq!(fired["alerts"][0]["source"], json!("ledger"));

        // inside its cooldown the rule is held back
        let again = evaluate_data(
            &graph,
            &scope(),
            &json!({"data": {"amount": 12500}, "now": "2026-01-02T09:30:00Z"}),
        )
        .expect("a watch");
        assert_eq!(again["triggered_count"], json!(0));

        // and once the hour is out it fires again
        let later = evaluate_data(
            &graph,
            &scope(),
            &json!({"data": {"amount": 12500}, "now": "2026-01-02T10:00:01Z"}),
        )
        .expect("a watch");
        assert_eq!(later["triggered_count"], json!(1));
        let rules = alert_rules_held(&graph, &scope(), &json!({})).expect("rules");
        assert_eq!(rules[0]["trigger_count"], json!(2));
        assert_eq!(rules[0]["last_triggered"], json!("2026-01-02T10:00:01Z"));
    }

    #[test]
    fn a_condition_the_engine_does_not_understand_is_not_a_match() {
        let graph = MemoryGraph::new();
        a_rule(
            &graph,
            json!({"type": "threshold", "field": "amount", "operator": "roughly", "value": 1}),
            0,
        );
        a_rule(
            &graph,
            json!({"type": "threshold", "field": "amount", "operator": "gt", "value": 1}),
            0,
        );
        a_rule(&graph, json!({"type": "guesswork"}), 0);
        // the unknown operator and the unknown kind do not match, the plain threshold does
        let fired = evaluate_data(
            &graph,
            &scope(),
            &json!({"data": {"amount": 5}, "now": "2026-01-02T09:00:00Z"}),
        )
        .expect("a watch");
        assert_eq!(fired["triggered_count"], json!(1));
        // a value that is not a number is not a match either
        let text_value = evaluate_data(
            &graph,
            &scope(),
            &json!({"data": {"amount": "plenty"}, "now": "2026-01-02T09:00:01Z"}),
        )
        .expect("a watch");
        assert_eq!(text_value["triggered_count"], json!(0));
    }

    #[test]
    fn a_velocity_pattern_and_a_deadline_both_watch_what_they_say() {
        let graph = MemoryGraph::new();
        a_rule(
            &graph,
            json!({"type": "pattern", "pattern_type": "velocity", "threshold": 10}),
            0,
        );
        a_rule(&graph, json!({"type": "deadline", "days_before": 3}), 0);
        let busy = evaluate_data(
            &graph,
            &scope(),
            &json!({"data": {"transaction_count": 11, "time_window_minutes": 30},
                    "now": "2026-01-02T09:00:00Z"}),
        )
        .expect("a watch");
        assert_eq!(busy["triggered_count"], json!(1));
        // the same movements spread over a longer window are not a velocity
        let slow = evaluate_data(
            &graph,
            &scope(),
            &json!({"data": {"transaction_count": 11, "time_window_minutes": 90},
                    "now": "2026-01-02T09:00:01Z"}),
        )
        .expect("a watch");
        assert_eq!(slow["triggered_count"], json!(0));

        let due = evaluate_data(
            &graph,
            &scope(),
            &json!({"data": {"deadline": "2026-01-04", "now": "2026-01-02T09:00:00Z"},
                    "now": "2026-01-02T09:00:02Z"}),
        )
        .expect("a watch");
        assert_eq!(due["triggered_count"], json!(1));
        // a deadline already gone is not approaching
        let gone = evaluate_data(
            &graph,
            &scope(),
            &json!({"data": {"deadline": "2025-12-30", "now": "2026-01-02T09:00:00Z"},
                    "now": "2026-01-02T09:00:03Z"}),
        )
        .expect("a watch");
        assert_eq!(gone["triggered_count"], json!(0));
    }

    #[test]
    fn a_switched_off_rule_is_not_watched_but_can_still_be_shouted_through() {
        let graph = MemoryGraph::new();
        let rule = a_rule(
            &graph,
            json!({"type": "threshold", "field": "amount", "operator": "gt", "value": 1}),
            3600,
        );
        replace_rule(
            &graph,
            &scope(),
            field(&rule, "id"),
            &json!({"name": "Large payment", "category": "fraud", "severity": "high",
                    "condition": {"type": "threshold", "field": "amount", "operator": "gt", "value": 1},
                    "enabled": false, "cooldown_seconds": 3600, "updated_at": "2026-01-02T09:00:00Z"}),
        )
        .expect("a replacement");
        let watched = evaluate_data(
            &graph,
            &scope(),
            &json!({"data": {"amount": 5}, "now": "2026-01-03T09:00:00Z"}),
        )
        .expect("a watch");
        assert_eq!(watched["triggered_count"], json!(0));

        let shouted = trigger_rule(
            &graph,
            &scope(),
            field(&rule, "id"),
            &json!({"message": "Close the ledger", "now": "2026-01-03T09:00:00Z"}),
        )
        .expect("an alert");
        assert_eq!(shouted["message"], json!("Close the ledger"));
        assert_eq!(shouted["severity"], json!("high"));
        assert_eq!(shouted["source"], json!("internal"));
    }

    #[test]
    fn replacing_a_rule_keeps_how_often_it_has_fired() {
        let graph = MemoryGraph::new();
        let rule = a_rule(
            &graph,
            json!({"type": "threshold", "field": "amount", "operator": "gt", "value": 1}),
            0,
        );
        evaluate_data(
            &graph,
            &scope(),
            &json!({"data": {"amount": 5}, "now": "2026-01-02T09:00:00Z"}),
        )
        .expect("a watch");
        let replaced = replace_rule(
            &graph,
            &scope(),
            field(&rule, "id"),
            &json!({"name": "Large payment, renamed", "category": "fraud", "severity": "low",
                    "condition": {"type": "threshold", "field": "amount", "operator": "gt", "value": 2},
                    "cooldown_seconds": 60, "updated_at": "2026-01-03T09:00:00Z"}),
        )
        .expect("a replacement");
        assert_eq!(replaced["name"], json!("Large payment, renamed"));
        // the bookkeeping is not the caller's to rewrite
        assert_eq!(replaced["trigger_count"], json!(1));
        assert_eq!(replaced["last_triggered"], json!("2026-01-02T09:00:00Z"));
        assert_eq!(replaced["created_at"], json!("2026-01-01T09:00:00Z"));
    }

    #[test]
    fn an_alert_is_settled_without_anyone_asking_where_it_stood() {
        let graph = MemoryGraph::new();
        let made = create_alert(
            &graph,
            &scope(),
            &json!({"rule_id": "r-1", "title": "Cash variance", "message": "Drawer short",
                    "severity": "medium", "category": "financial", "source": "cashbook",
                    "created_at": "2026-01-02T09:00:00Z"}),
        )
        .expect("an alert");
        assert_eq!(made["status"], json!("active"));
        let id = field(&made, "id").to_string();
        let acknowledged = acknowledge_alert(
            &graph,
            &scope(),
            &id,
            &json!({"acknowledged_at": "2026-01-02T10:00:00Z"}),
        )
        .expect("acknowledged");
        assert_eq!(acknowledged["status"], json!("acknowledged"));
        assert_eq!(
            acknowledged["acknowledged_at"],
            json!("2026-01-02T10:00:00Z")
        );
        // resolving an acknowledged alert is not refused: the shipped endpoints never looked
        let resolved = resolve_alert(
            &graph,
            &scope(),
            &id,
            &json!({"resolved_at": "2026-01-02T11:00:00Z"}),
        )
        .expect("resolved");
        assert_eq!(resolved["status"], json!("resolved"));
        let dismissed = dismiss_alert(&graph, &scope(), &id).expect("dismissed");
        assert_eq!(dismissed["status"], json!("dismissed"));
    }

    #[test]
    fn the_count_of_alerts_and_rules_is_the_callers_own() {
        let graph = MemoryGraph::new();
        a_rule(
            &graph,
            json!({"type": "threshold", "field": "amount", "operator": "gt", "value": 1}),
            0,
        );
        create_alert(
            &graph,
            &scope(),
            &json!({"rule_id": "r-1", "title": "One", "message": "One", "severity": "high",
                    "category": "fraud", "source": "system", "created_at": "2026-01-02T09:00:00Z"}),
        )
        .expect("an alert");
        let counted = alerts_counted(&graph, &scope()).expect("a count");
        assert_eq!(counted["total_alerts"], json!(1));
        assert_eq!(counted["total_rules"], json!(1));
        assert_eq!(counted["enabled_rules"], json!(1));
        assert_eq!(counted["by_severity"]["high"], json!(1));
        // nobody is listening, because there are no live sockets here
        assert_eq!(counted["active_connections"], json!(0));

        let theirs = alerts_counted(&graph, &BookScope::new("user-2", "book-2")).expect("a count");
        assert_eq!(theirs["total_alerts"], json!(0));
    }
}
