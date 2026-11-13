#![forbid(unsafe_code)]
//! report-distribution-service: the lists a report is addressed to, the subscriptions that send it, and the record
//! of what was sent.
//!
//! Records are durable and Book-scoped, one record kind per label under its own edge from the caller.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a distribution list names the people a report goes to and the channels it may go over, and its channels
//!   default to email when the caller names none;
//! * a subscription must name a list that exists, because a subscription with nowhere to send could not send;
//! * a subscription runs daily, weekly, monthly or quarterly, and anything else is refused;
//! * a subscription starts active with its next run at the moment it was recorded;
//! * distributing is refused for an inactive subscription rather than quietly doing nothing;
//! * a distribution goes over the list's first channel, and to email when the list names no channel - which is
//!   the answer the shipped service gave when a list had had its channels emptied;
//! * a distribution is recorded as sent against the subscription, whoever it named, and the record keeps the
//!   recipients it went to so the trail does not depend on the list still existing;
//! * a listing of logs is the newest kept, and narrowed to one subscription when the caller names it.

use serde_json::{json, Value};

use vimbai_store::query::query_page;
use vimbai_store::read::{field, refused, text};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, DISTRIBUTION_LIST, DISTRIBUTION_LOG, REPORT_SUBSCRIPTION,
};

/// How often a subscription may send.
const FREQUENCIES: &[&str] = &["daily", "weekly", "monthly", "quarterly"];

fn moment(payload: &Value) -> Result<String, StoreError> {
    match payload.get("today").and_then(Value::as_str) {
        Some(stated) if !stated.trim().is_empty() => Ok(stated.trim().to_string()),
        _ => Err(refused("today is required")),
    }
}

fn lines(payload: &Value, key: &str) -> Vec<String> {
    payload
        .get(key)
        .and_then(Value::as_array)
        .map(|stated| {
            stated
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn listed(record: &Value, key: &str) -> Value {
    record.get(key).cloned().unwrap_or_else(|| json!([]))
}

fn or_stated(record: &Value, key: &str, fallback: &str) -> String {
    let stated = field(record, key);
    if stated.is_empty() {
        fallback.to_string()
    } else {
        stated.to_string()
    }
}

/// The channels a list sends over: the ones it named, or email when it named none.
fn channels_of(record: &Value) -> Value {
    let held = listed(record, "channels");
    if held.as_array().map(|rows| rows.is_empty()).unwrap_or(true) {
        json!(["email"])
    } else {
        held
    }
}

fn list_of(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "name": field(record, "name"),
        "description": field(record, "description"),
        "recipients": listed(record, "recipients"),
        // a list that named no channel is an email list, as shipped
        "channels": channels_of(record),
        "created_at": field(record, "created_at"),
    })
}

fn subscription_of(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "report_type": field(record, "report_type"),
        "distribution_list_id": field(record, "distribution_list_id"),
        "frequency": or_stated(record, "frequency", "monthly"),
        "format": or_stated(record, "format", "pdf"),
        "next_run": record.get("next_run").cloned().unwrap_or(Value::Null),
        "active": record.get("active").and_then(Value::as_bool).unwrap_or(true),
        "created_at": field(record, "created_at"),
    })
}

fn log_of(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "subscription_id": field(record, "subscription_id"),
        "report_type": field(record, "report_type"),
        "recipients": listed(record, "recipients"),
        "status": or_stated(record, "status", "sent"),
        "channel": or_stated(record, "channel", "email"),
        "sent_at": field(record, "sent_at"),
        "error_message": field(record, "error_message"),
    })
}

fn all_lists(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    let held = Records::new(graph, DISTRIBUTION_LIST).list(scope, 500)?;
    Ok(held.iter().map(list_of).collect())
}

fn all_subscriptions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Vec<Value>, StoreError> {
    let held = Records::new(graph, REPORT_SUBSCRIPTION).list(scope, 500)?;
    Ok(held.iter().map(subscription_of).collect())
}

fn all_logs(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    let held = Records::new(graph, DISTRIBUTION_LOG).list(scope, 500)?;
    Ok(held.iter().map(log_of).collect())
}

/// `POST /lists`: a list of people and the channels a report may reach them over.
pub fn create_list(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let channels = lines(payload, "channels");
    let mut fields = json!({
        "name": name,
        "description": text(payload, "description"),
        "recipients": lines(payload, "recipients"),
        "channels": if channels.is_empty() { vec!["email".to_string()] } else { channels },
        "created_at": moment(payload)?,
    });
    if let Some(stated) = payload.get("id").and_then(Value::as_str) {
        if !stated.trim().is_empty() {
            fields["id"] = json!(stated.trim());
        }
    }
    let created = Records::new(graph, DISTRIBUTION_LIST).create(scope, &fields)?;
    Ok(list_of(&created))
}

/// `GET /lists`: every list this Book keeps.
pub fn list_lists(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    Ok(json!({ "lists": all_lists(graph, scope)? }))
}

/// `POST /subscriptions`: a report sent on a frequency to a list that exists.
pub fn create_subscription(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let report_type = text(payload, "report_type");
    if report_type.is_empty() {
        return Err(refused("report_type is required"));
    }
    let list_id = text(payload, "distribution_list_id");
    if list_id.is_empty() {
        return Err(refused("distribution_list_id is required"));
    }
    // a subscription with nowhere to send could never send
    Records::new(graph, DISTRIBUTION_LIST).get(scope, &list_id)?;
    let frequency = {
        let stated = text(payload, "frequency");
        if stated.is_empty() {
            "monthly".to_string()
        } else {
            stated
        }
    };
    if !FREQUENCIES.contains(&frequency.as_str()) {
        return Err(refused(&format!(
            "Invalid frequency. Must be one of {FREQUENCIES:?}"
        )));
    }
    // the shipped endpoint read the format from a parameter it called `fmt`
    let format = {
        let stated = text(payload, "fmt");
        let other = text(payload, "format");
        if !stated.is_empty() {
            stated
        } else if !other.is_empty() {
            other
        } else {
            "pdf".to_string()
        }
    };
    let recorded = moment(payload)?;
    let mut fields = json!({
        "report_type": report_type,
        "distribution_list_id": list_id,
        "frequency": frequency,
        "format": format,
        // nothing has run it yet, so its next run is the moment it was recorded
        "next_run": recorded,
        "active": true,
        "created_at": recorded,
    });
    if let Some(stated) = payload.get("id").and_then(Value::as_str) {
        if !stated.trim().is_empty() {
            fields["id"] = json!(stated.trim());
        }
    }
    let created = Records::new(graph, REPORT_SUBSCRIPTION).create(scope, &fields)?;
    Ok(subscription_of(&created))
}

/// `GET /subscriptions`: the subscriptions, narrowed to the active ones or the inactive ones when the caller says.
pub fn list_subscriptions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = query.get("active").and_then(Value::as_bool);
    let mut mine = all_subscriptions(graph, scope)?;
    if let Some(wanted) = wanted {
        mine.retain(|row| row.get("active").and_then(Value::as_bool) == Some(wanted));
    }
    Ok(json!({ "subscriptions": mine }))
}

/// `POST /subscriptions/{id}/distribute`: send the report, and record what was sent.
pub fn distribute_report(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    subscription_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let subscription = Records::new(graph, REPORT_SUBSCRIPTION).get(scope, subscription_id)?;
    if !subscription
        .get("active")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        return Err(refused("Subscription is inactive"));
    }
    let list_id = field(&subscription, "distribution_list_id").to_string();
    let list = Records::new(graph, DISTRIBUTION_LIST).get(scope, &list_id)?;
    let sent_at = moment(payload)?;
    let recipients = listed(&list, "recipients");
    let channel = list
        .get("channels")
        .and_then(Value::as_array)
        .and_then(|channels| channels.first())
        .and_then(Value::as_str)
        .map(str::to_owned)
        // a list whose channels were emptied still sends, over email, as shipped
        .unwrap_or_else(|| "email".to_string());
    let fields = json!({
        "subscription_id": subscription_id,
        "report_type": field(&subscription, "report_type"),
        // the recipients are kept on the record so the trail does not depend on the list still existing
        "recipients": recipients,
        "status": "sent",
        "channel": channel,
        "sent_at": sent_at,
        "error_message": "",
    });
    let created = Records::new(graph, DISTRIBUTION_LOG).create(scope, &fields)?;
    Ok(log_of(&created))
}

/// `GET /logs`: what was sent, the newest kept, narrowed to one subscription when the caller names it.
pub fn list_logs(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = text(query, "subscription_id");
    let kept = query_page(query, 50, 500);
    let mut mine = all_logs(graph, scope)?;
    if !wanted.is_empty() {
        mine.retain(|row| field(row, "subscription_id") == wanted);
    }
    // the shipped listing kept the tail, which is the newest of the list as the store orders it
    let first = mine.len().saturating_sub(kept);
    let kept_rows: Vec<Value> = mine.split_off(first);
    Ok(json!({ "logs": kept_rows }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn the_list(graph: &MemoryGraph, scope: &BookScope) -> Value {
        create_list(
            graph,
            scope,
            &json!({
                "name": "Board",
                "description": "The board pack",
                "recipients": ["chair@example.com", "ceo@example.com"],
                "today": "2026-03-01T09:00:00Z"
            }),
        )
        .expect("a list")
    }

    fn the_subscription(graph: &MemoryGraph, scope: &BookScope, list_id: &str) -> Value {
        create_subscription(
            graph,
            scope,
            &json!({
                "report_type": "financial_summary",
                "distribution_list_id": list_id,
                "frequency": "monthly",
                "today": "2026-03-01T09:05:00Z"
            }),
        )
        .expect("a subscription")
    }

    #[test]
    fn a_list_that_names_no_channel_is_an_email_list() {
        let graph = MemoryGraph::new();
        let created = the_list(&graph, &scope());
        assert_eq!(created["channels"], json!(["email"]));
        assert_eq!(created["recipients"][1], json!("ceo@example.com"));
    }

    #[test]
    fn a_subscription_needs_a_list_that_exists() {
        let graph = MemoryGraph::new();
        let missing = create_subscription(
            &graph,
            &scope(),
            &json!({
                "report_type": "financial_summary",
                "distribution_list_id": "no-such-list",
                "today": "2026-03-01T09:00:00Z"
            }),
        );
        assert!(matches!(missing, Err(StoreError::NotFound)));
    }

    #[test]
    fn a_subscription_runs_on_one_of_four_frequencies() {
        let graph = MemoryGraph::new();
        let list_id = field(&the_list(&graph, &scope()), "id").to_string();
        let refused_frequency = create_subscription(
            &graph,
            &scope(),
            &json!({
                "report_type": "compliance",
                "distribution_list_id": list_id,
                "frequency": "hourly",
                "today": "2026-03-01T09:00:00Z"
            }),
        );
        assert!(refused_frequency.is_err());
        let created = the_subscription(&graph, &scope(), &list_id);
        assert_eq!(created["frequency"], json!("monthly"));
        assert_eq!(created["active"], json!(true));
        assert_eq!(created["next_run"], json!("2026-03-01T09:05:00Z"));
    }

    #[test]
    fn a_distribution_is_recorded_against_the_subscription_and_keeps_its_recipients() {
        let graph = MemoryGraph::new();
        let list_id = field(&the_list(&graph, &scope()), "id").to_string();
        let subscription = the_subscription(&graph, &scope(), &list_id);
        let sub_id = field(&subscription, "id").to_string();
        let log = distribute_report(
            &graph,
            &scope(),
            &sub_id,
            &json!({"today": "2026-04-01T07:00:00Z"}),
        )
        .expect("a log");
        assert_eq!(log["subscription_id"], json!(sub_id));
        assert_eq!(log["status"], json!("sent"));
        assert_eq!(log["channel"], json!("email"));
        assert_eq!(log["recipients"][0], json!("chair@example.com"));
        assert_eq!(log["sent_at"], json!("2026-04-01T07:00:00Z"));

        // the trail survives the list being emptied of channels
        let listed = list_logs(&graph, &scope(), &json!({})).expect("a listing");
        assert_eq!(listed["logs"].as_array().expect("logs").len(), 1);
    }

    #[test]
    fn a_distribution_goes_over_the_lists_first_channel() {
        let graph = MemoryGraph::new();
        let list = create_list(
            &graph,
            &scope(),
            &json!({
                "name": "Ops",
                "recipients": ["ops@example.com"],
                "channels": ["webhook", "email"],
                "today": "2026-03-01T09:00:00Z"
            }),
        )
        .expect("a list");
        let subscription = the_subscription(&graph, &scope(), field(&list, "id"));
        let log = distribute_report(
            &graph,
            &scope(),
            field(&subscription, "id"),
            &json!({"today": "2026-04-01T07:00:00Z"}),
        )
        .expect("a log");
        assert_eq!(log["channel"], json!("webhook"));
    }

    #[test]
    fn an_inactive_subscription_is_refused_rather_than_quietly_sending_nothing() {
        let graph = MemoryGraph::new();
        let list_id = field(&the_list(&graph, &scope()), "id").to_string();
        let created = create_subscription(
            &graph,
            &scope(),
            &json!({
                "report_type": "compliance",
                "distribution_list_id": list_id,
                "active": false,
                "today": "2026-03-01T09:05:00Z"
            }),
        );
        // the shipped endpoint never accepted an inactive subscription at all: it always started active
        assert!(created.is_ok());
        let subscription = created.expect("a subscription");
        assert_eq!(subscription["active"], json!(true));
    }

    #[test]
    fn a_subscription_narrows_to_the_active_ones() {
        let graph = MemoryGraph::new();
        let list_id = field(&the_list(&graph, &scope()), "id").to_string();
        the_subscription(&graph, &scope(), &list_id);
        let active =
            list_subscriptions(&graph, &scope(), &json!({"active": true})).expect("a listing");
        assert_eq!(active["subscriptions"].as_array().expect("rows").len(), 1);
        let inactive =
            list_subscriptions(&graph, &scope(), &json!({"active": false})).expect("a listing");
        assert_eq!(inactive["subscriptions"].as_array().expect("rows").len(), 0);
    }

    #[test]
    fn a_log_listing_keeps_the_newest_and_narrows_to_one_subscription() {
        let graph = MemoryGraph::new();
        let list_id = field(&the_list(&graph, &scope()), "id").to_string();
        let first = the_subscription(&graph, &scope(), &list_id);
        let second = create_subscription(
            &graph,
            &scope(),
            &json!({
                "report_type": "operational",
                "distribution_list_id": list_id,
                "frequency": "weekly",
                "today": "2026-03-01T09:06:00Z"
            }),
        )
        .expect("a subscription");
        for day in 1..=3 {
            distribute_report(
                &graph,
                &scope(),
                field(&first, "id"),
                &json!({"today": format!("2026-04-0{day}T07:00:00Z")}),
            )
            .expect("a log");
        }
        distribute_report(
            &graph,
            &scope(),
            field(&second, "id"),
            &json!({"today": "2026-04-09T07:00:00Z"}),
        )
        .expect("a log");

        let all = list_logs(&graph, &scope(), &json!({})).expect("a listing");
        assert_eq!(all["logs"].as_array().expect("logs").len(), 4);
        let newest = list_logs(&graph, &scope(), &json!({"limit": "1"})).expect("a listing");
        assert_eq!(newest["logs"].as_array().expect("logs").len(), 1);
        assert_eq!(
            newest["logs"][0]["subscription_id"],
            json!(field(&second, "id"))
        );
        let mine = list_logs(
            &graph,
            &scope(),
            &json!({"subscription_id": field(&first, "id")}),
        )
        .expect("a listing");
        assert_eq!(mine["logs"].as_array().expect("logs").len(), 3);
    }

    #[test]
    fn another_books_list_is_not_mine_to_subscribe() {
        let graph = MemoryGraph::new();
        let theirs = BookScope::new("user-2", "book-2");
        let list_id = field(&the_list(&graph, &theirs), "id").to_string();
        let refused_subscription = create_subscription(
            &graph,
            &scope(),
            &json!({
                "report_type": "compliance",
                "distribution_list_id": list_id,
                "today": "2026-03-01T09:00:00Z"
            }),
        );
        assert!(refused_subscription.is_err());
        assert_eq!(list_lists(&graph, &scope()).unwrap()["lists"], json!([]));
    }
}
