#![forbid(unsafe_code)]
//! subscription-plans-service: the plans a Book sells on, the subscriptions taken against them, and what a change
//! of plan would cost.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a plan keeps the tier, name and monthly price it is given and defaults to 5 users, 1 company and 1,000 calls a
//!   month when nothing else is said;
//! * a subscription needs a plan the CALLER holds - another Book's plan is simply not found - and takes the plan's
//!   tier rather than a tier of its own, with its period running 30, 90 or 365 days from the day it starts;
//! * the upgrade preview is PURE and does not read a stored plan: it prices the four tiers from a fixed table
//!   (0, 49, 199 and 999 a month), which is what the shipped endpoint did, so a plan a Book priced itself does not
//!   change what an upgrade is quoted at. Proration is the daily difference over the days left in the period,
//!   rounded to cents, and a period end that cannot be read prorates nothing.

use serde_json::{json, Value};

use vimbai_common::{dates, Money};
use vimbai_store::read::{decimal, field, or_default, refused, text, whole};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A plan a Book sells on.
pub const PLAN: RecordKind = RecordKind::new("Plan", "OWNS_PLAN");

/// A subscription a company holds.
pub const SUBSCRIPTION: RecordKind = RecordKind::new("Subscription", "OWNS_SUBSCRIPTION");

/// The monthly price of each tier, as the shipped upgrade preview knows them, kept as exact decimals so the proration
/// is reckoned in money rather than in floats.
pub const TIER_PRICES: [(&str, &str); 4] = [
    ("free", "0"),
    ("basic", "49"),
    ("professional", "199"),
    ("enterprise", "999"),
];

fn price_of(tier: &str) -> Money {
    TIER_PRICES
        .iter()
        .find(|(name, _)| *name == tier)
        .and_then(|(_, price)| Money::parse(price).ok())
        .unwrap_or_else(Money::zero)
}

/// Money written back as a JSON number, which is the shape the shipped answer carried.
fn as_number(amount: &Money) -> Value {
    serde_json::from_str(&amount.to_fixed(2)).unwrap_or_else(|_| json!(0))
}

/// An amount read as its exact decimal text and written back as a JSON number, so a plan priced 199 is 199 rather
/// than a float a hundredth of a cent out.
fn amount(value: &Value, key: &str, fallback: &str) -> Value {
    serde_json::from_str(&decimal(value, key, fallback)).unwrap_or_else(|_| json!(0))
}

/// A count that keeps the service's own default when nothing was said, which is what the shipped model did.
fn count(value: &Value, key: &str, fallback: i64) -> i64 {
    match value.get(key) {
        None | Some(Value::Null) => fallback,
        Some(_) => i64::from(whole(value, key)),
    }
}

fn plan_answer(record: &Value) -> Value {
    let tier = or_default(record, "tier", "basic");
    let features = match record.get("features") {
        Some(Value::Array(held)) => Value::Array(
            held.iter()
                .filter_map(|one| one.as_str().map(|name| json!(name)))
                .collect(),
        ),
        _ => json!([]),
    };
    json!({
        "id": field(record, "id"),
        "tier": tier,
        "name": field(record, "name"),
        "price_monthly": amount(record, "price_monthly", "0"),
        "features": features,
        "max_users": count(record, "max_users", 5),
        "max_companies": count(record, "max_companies", 1),
        "api_calls_per_month": count(record, "api_calls_per_month", 1000),
    })
}

fn subscription_answer(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "company_id": field(record, "company_id"),
        "plan_id": field(record, "plan_id"),
        "tier": or_default(record, "tier", "basic"),
        "billing_cycle": or_default(record, "billing_cycle", "monthly"),
        "start_date": field(record, "start_date"),
        "status": or_default(record, "status", "active"),
        "current_period_end": field(record, "current_period_end"),
    })
}

/// `POST /plans`: record a plan.
pub fn create_plan(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    if text(payload, "name").is_empty() {
        return Err(refused("name is required"));
    }
    let tier = {
        let held = text(payload, "tier");
        if held.is_empty() {
            "basic".to_string()
        } else {
            held
        }
    };
    let features = match payload.get("features") {
        Some(Value::Array(held)) => Value::Array(held.clone()),
        _ => json!([]),
    };
    let fields = json!({
        "id": vimbai_common::ids::service_id(),
        "tier": tier,
        "name": text(payload, "name"),
        "price_monthly": amount(payload, "price_monthly", "0"),
        "features": features,
        "max_users": count(payload, "max_users", 5),
        "max_companies": count(payload, "max_companies", 1),
        "api_calls_per_month": count(payload, "api_calls_per_month", 1000),
        "created_at": text(payload, "created_at"),
    });
    let created = Records::new(graph, PLAN).create(scope, &fields)?;
    Ok(plan_answer(&created))
}

/// `GET /plans`: the caller's plans, in the order they were recorded.
pub fn list_plans(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let mut held = Records::new(graph, PLAN).list(scope, 500)?;
    held.sort_by_key(|plan| field(plan, "created_at").to_string());
    Ok(Value::Array(held.iter().map(plan_answer).collect()))
}

/// `POST /subscribe`: take a subscription against one of the caller's plans.
pub fn subscribe(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = text(payload, "company_id");
    if company_id.is_empty() {
        return Err(refused("company_id is required"));
    }
    let plan_id = text(payload, "plan_id");
    let plan = Records::new(graph, PLAN).get(scope, &plan_id)?;
    let cycle = {
        let held = text(payload, "cycle");
        if held.is_empty() {
            "monthly".to_string()
        } else {
            held
        }
    };
    let today = text(payload, "today");
    if dates::day_of(&today).is_none() {
        return Err(refused("today is required, written as a date"));
    }
    let days = match cycle.as_str() {
        "quarterly" => 90,
        "annual" => 365,
        _ => 30,
    };
    let end = dates::date_after(&today, days).unwrap_or_else(|| today.clone());
    // the tier comes from the plan, not from the caller
    let tier = or_default(&plan, "tier", "basic");
    let fields = json!({
        "id": vimbai_common::ids::service_id(),
        "company_id": company_id,
        "plan_id": plan_id,
        "tier": tier,
        "billing_cycle": cycle,
        "start_date": today,
        "status": "active",
        "current_period_end": end,
        "created_at": text(payload, "created_at"),
    });
    let created = Records::new(graph, SUBSCRIPTION).create(scope, &fields)?;
    Ok(subscription_answer(&created))
}

/// `GET /subscriptions/{company}`: a company's subscriptions.
pub fn subscriptions_for_company(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, SUBSCRIPTION).list(scope, 500)?;
    let mine: Vec<Value> = held
        .iter()
        .filter(|subscription| field(subscription, "company_id") == company_id)
        .map(subscription_answer)
        .collect();
    Ok(Value::Array(mine))
}

/// `POST /upgrade`: what moving between two tiers would cost, from the fixed table and the days left.
pub fn preview_upgrade(payload: &Value) -> Result<Value, StoreError> {
    let company_id = text(payload, "company_id");
    if company_id.is_empty() {
        return Err(refused("company_id is required"));
    }
    let today = text(payload, "today");
    if dates::day_of(&today).is_none() {
        return Err(refused("today is required, written as a date"));
    }
    let from = text(payload, "current_plan");
    let to = text(payload, "target_plan");
    let current_price = price_of(&from);
    let target_price = price_of(&to);
    let prorate = match payload.get("prorate") {
        Some(Value::Bool(asked)) => *asked,
        _ => true,
    };
    // the daily difference over the days left, and nothing at all when the period end cannot be read
    let proration = if prorate {
        match dates::days_between(&text(payload, "current_period_end"), &today) {
            Some(left) => {
                let remaining = left.max(0);
                // the difference a day, over the days left: multiplied out first so nothing is lost to a division
                let difference = target_price.minus(&current_price);
                let change = difference
                    .times(&Money::parse(&remaining.to_string()).unwrap_or_else(|_| Money::zero()))
                    .over(&Money::parse("30").expect("thirty days in a month"));
                change.round(2)
            }
            None => Money::zero(),
        }
    } else {
        Money::zero()
    };
    Ok(json!({
        "company_id": company_id,
        "current_plan": from,
        "target_plan": to,
        "proration_amount": as_number(&proration),
        "effective_date": today,
        "new_billing_amount": as_number(&target_price),
        // the shipped answer always says monthly
        "cycle": "monthly",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn a_plan(graph: &MemoryGraph, name: &str, tier: &str, created: &str) -> Value {
        create_plan(
            graph,
            &scope(),
            &json!({
                "tier": tier,
                "name": name,
                "price_monthly": 199,
                "features": ["ledgers", "reports"],
                "created_at": created,
            }),
        )
        .expect("a plan")
    }

    #[test]
    fn a_plan_keeps_what_it_is_given_and_defaults_its_limits() {
        let graph = MemoryGraph::new();
        let made = a_plan(
            &graph,
            "Professional",
            "professional",
            "2026-01-01T09:00:00Z",
        );
        assert_eq!(made["tier"], json!("professional"));
        // the price is read exactly and written back exactly: 199, not a float a hundredth of a cent out
        assert_eq!(made["price_monthly"], json!(199));
        assert_eq!(made["features"], json!(["ledgers", "reports"]));
        // the shipped defaults for a plan that says nothing about its limits
        assert_eq!(made["max_users"], json!(5));
        assert_eq!(made["max_companies"], json!(1));
        assert_eq!(made["api_calls_per_month"], json!(1000));
    }

    #[test]
    fn a_subscription_takes_the_plans_tier_and_runs_its_cycle() {
        let graph = MemoryGraph::new();
        let plan = a_plan(&graph, "Basic", "basic", "2026-01-01T09:00:00Z");
        let taken = subscribe(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1",
                "plan_id": field(&plan, "id"),
                "cycle": "quarterly",
                "today": "2026-03-01",
            }),
        )
        .expect("a subscription");
        // the tier comes from the plan, not from the caller
        assert_eq!(taken["tier"], json!("basic"));
        assert_eq!(taken["status"], json!("active"));
        assert_eq!(taken["start_date"], json!("2026-03-01"));
        assert_eq!(taken["current_period_end"], json!("2026-05-30"));

        let listed = subscriptions_for_company(&graph, &scope(), "co-1").expect("a listing");
        assert_eq!(listed.as_array().map(|held| held.len()), Some(1));
        let other = subscriptions_for_company(&graph, &scope(), "co-2").expect("a listing");
        assert_eq!(other, json!([]));
    }

    #[test]
    fn another_books_plan_cannot_be_subscribed_to() {
        let graph = MemoryGraph::new();
        let plan = a_plan(&graph, "Basic", "basic", "2026-01-01T09:00:00Z");
        let refused_take = subscribe(
            &graph,
            &BookScope::new("user-2", "book-2"),
            &json!({"company_id": "co-1", "plan_id": field(&plan, "id"), "today": "2026-03-01"}),
        );
        assert!(refused_take.is_err());
    }

    #[test]
    fn an_upgrade_is_quoted_from_the_fixed_table_and_the_days_left() {
        // thirty days left, moving from basic at 49 to professional at 199: 5 a day for a month
        let quoted = preview_upgrade(&json!({
            "company_id": "co-1",
            "current_plan": "basic",
            "target_plan": "professional",
            "current_period_end": "2026-04-01",
            "today": "2026-03-02"
        }))
        .expect("a quote");
        assert_eq!(quoted["proration_amount"], json!(150.0));
        // a price is written with its cents, since it is money that is being quoted
        assert_eq!(quoted["new_billing_amount"], json!(199.0));
        assert_eq!(quoted["effective_date"], json!("2026-03-02"));
        assert_eq!(quoted["cycle"], json!("monthly"));

        // a period that has already run out prorates nothing, and neither does an unreadable date
        let late = preview_upgrade(&json!({
            "company_id": "co-1",
            "current_plan": "basic",
            "target_plan": "enterprise",
            "current_period_end": "2026-01-01",
            "today": "2026-03-02"
        }))
        .expect("a quote");
        assert_eq!(late["proration_amount"], json!(0.0));

        let broken = preview_upgrade(&json!({
            "company_id": "co-1",
            "current_plan": "basic",
            "target_plan": "enterprise",
            "current_period_end": "not a date",
            "today": "2026-03-02"
        }))
        .expect("a quote");
        assert_eq!(broken["proration_amount"], json!(0.0));
        assert_eq!(broken["new_billing_amount"], json!(999.0));
    }

    #[test]
    fn an_upgrade_can_decline_to_prorate() {
        let quoted = preview_upgrade(&json!({
            "company_id": "co-1",
            "current_plan": "basic",
            "target_plan": "professional",
            "current_period_end": "2026-04-01",
            "today": "2026-03-02",
            "prorate": false
        }))
        .expect("a quote");
        assert_eq!(quoted["proration_amount"], json!(0.0));
    }
}
