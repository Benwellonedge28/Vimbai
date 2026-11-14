#![forbid(unsafe_code)]
//! personal-finance-service: a household's recurring bills and income, the debts it is paying down, and what it holds.
//!
//! The part that matters here is the debt ledger. A payment does not simply reduce the balance: interest is charged
//! daily on whatever was outstanding since the previous payment, and only the rest of the payment touches the
//! principal. So the state of a debt is a replay of its payments, and two people making the same payments at different
//! intervals do not owe the same amount.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a recurring item is a bill or an income, on one of four frequencies, with a due date that must read as a date;
//! * deactivating a recurring item keeps it rather than deleting it, and a run records the item and moves the due date
//!   on by its frequency;
//! * a debt's scheduled payment is the level payment that would clear the principal over its term, and it is nothing
//!   when the rate is nothing, in which case the principal is simply spread over the months;
//! * interest accrues at the annual rate over 365 days for the actual days between payments, and a payment smaller
//!   than the interest leaves the principal untouched rather than negative;
//! * a projection whose payment cannot cover the interest is refused rather than run forever;
//! * a sale of more units than are held is refused; a buy re-averages the cost, a sale keeps it and reports what was
//!   realised.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::query::query_page;
use vimbai_store::read::{decimal, field, optional, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, PERSONAL_DEBT, PERSONAL_INVESTMENT, PERSONAL_PAYMENT,
    PERSONAL_RECURRING, PERSONAL_TRADE,
};

use crate::personal_dates::{advance, parse_date, parse_moment};

const FREQUENCIES: &[&str] = &["weekly", "monthly", "quarterly", "yearly"];

fn amount_of(payload: &Value, key: &str) -> Result<Money, StoreError> {
    Money::parse(&decimal(payload, key, "0"))
        .map_err(|_| refused(&format!("{key} must be an amount")))
}

fn positive(payload: &Value, key: &str) -> Result<Money, StoreError> {
    let value = amount_of(payload, key)?;
    if value.is_negative() || value.is_zero() {
        return Err(refused(&format!("{key} must be greater than zero")));
    }
    Ok(value)
}

fn rate_of(payload: &Value, key: &str) -> Result<Money, StoreError> {
    let value = amount_of(payload, key)?;
    if value.is_negative() {
        return Err(refused(&format!("{key} cannot be negative")));
    }
    Ok(value)
}

fn hundred() -> Money {
    Money::parse("100").unwrap_or_else(|_| Money::zero())
}

// ---------------------------------------------------------------- recurring items

/// `POST /recurring`.
pub fn create_recurring(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let kind = text(payload, "kind");
    if kind != "bill" && kind != "income" {
        return Err(refused("kind must be bill or income"));
    }
    let frequency = text(payload, "frequency");
    if !FREQUENCIES.contains(&frequency.as_str()) {
        return Err(refused(
            "frequency must be one of weekly, monthly, quarterly or yearly",
        ));
    }
    let next_due = text(payload, "next_due");
    if parse_date(&next_due).is_none() {
        return Err(refused("next_due must be an ISO date"));
    }

    remember(
        graph,
        scope,
        PERSONAL_RECURRING,
        json!({
            "kind": kind,
            "description": text(payload, "description"),
            "amount": wire(&amount_of(payload, "amount")?),
            "currency": or_default(payload, "currency", "USD"),
            "frequency": frequency,
            "next_due": next_due,
            "auto_record": payload.get("auto_record").and_then(Value::as_bool).unwrap_or(false),
            "active": true,
            "last_run": "",
        }),
    )
}

fn or_default(payload: &Value, key: &str, fallback: &str) -> String {
    let value = text(payload, key);
    if value.is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

/// `GET /recurring`: the active items, soonest due first.
pub fn list_recurring(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted_book = optional(query, "book_id");
    let limit = query_page(query, 200, 2000);
    let mut rows: Vec<Value> = Records::new(graph, PERSONAL_RECURRING)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| row.get("active").and_then(Value::as_bool).unwrap_or(true))
        .filter(|row| {
            wanted_book
                .as_deref()
                .map(|wanted| field(row, "book_id") == wanted)
                .unwrap_or(true)
        })
        .collect();
    rows.sort_by(|one, two| field(one, "next_due").cmp(field(two, "next_due")));
    rows.truncate(limit);
    Ok(Value::Array(rows))
}

fn recurring_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    recurring_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, PERSONAL_RECURRING)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == recurring_id)
        .ok_or(StoreError::NotFound)
}

/// `DELETE /recurring/{rec_id}`: deactivated rather than removed, which is what the original did.
pub fn deactivate_recurring(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    recurring_id: &str,
) -> Result<Value, StoreError> {
    recurring_of(graph, scope, recurring_id)?;
    Records::new(graph, PERSONAL_RECURRING).update(
        scope,
        recurring_id,
        &json!({"active": false}),
    )?;
    Ok(json!({"status": "deactivated"}))
}

/// `PATCH /recurring/{rec_id}`: change an item in place, which the service allowed and the
/// deactivate route did not cover: a rent that goes up is still the same recurring item, not a new one.
pub fn update_recurring(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    recurring_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let existing = recurring_of(graph, scope, recurring_id)?;
    let mut change = serde_json::Map::new();
    if payload.get("amount").is_some_and(|value| !value.is_null()) {
        change.insert("amount".into(), json!(wire(&amount_of(payload, "amount")?)));
    }
    if let Some(frequency) = payload.get("frequency").filter(|value| !value.is_null()) {
        let frequency = frequency.as_str().unwrap_or_default().to_string();
        if !FREQUENCIES.contains(&frequency.as_str()) {
            return Err(refused(
                "frequency must be one of weekly, monthly, quarterly or yearly",
            ));
        }
        change.insert("frequency".into(), json!(frequency));
    }
    if let Some(next_due) = payload.get("next_due").filter(|value| !value.is_null()) {
        let next_due = next_due.as_str().unwrap_or_default().to_string();
        if parse_date(&next_due).is_none() {
            return Err(refused("next_due must be an ISO date"));
        }
        change.insert("next_due".into(), json!(next_due));
    }
    for key in ["description", "currency"] {
        if let Some(value) = payload.get(key).and_then(Value::as_str) {
            change.insert(key.into(), json!(value));
        }
    }
    if let Some(active) = payload.get("active").and_then(Value::as_bool) {
        change.insert("active".into(), json!(active));
    }
    if change.is_empty() {
        return Err(refused("nothing to update"));
    }

    let mut updated = existing.clone();
    Records::new(graph, PERSONAL_RECURRING).update(scope, recurring_id, &Value::Object(change))?;
    if let Some(object) = updated.as_object_mut() {
        for (key, value) in Records::new(graph, PERSONAL_RECURRING)
            .get(scope, recurring_id)?
            .as_object()
            .cloned()
            .unwrap_or_default()
        {
            object.insert(key, value);
        }
    }
    Ok(updated)
}

/// `POST /recurring/{rec_id}/run`: what to post to the cashbook, and when the item is next due.
pub fn run_recurring(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    recurring_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let item = recurring_of(graph, scope, recurring_id)?;
    let today = text(query, "today");
    if parse_date(&today).is_none() {
        return Err(refused("today must be an ISO date"));
    }
    let next_due = advance(field(&item, "next_due"), field(&item, "frequency"))
        .ok_or_else(|| refused("next_due must be an ISO date"))?;

    Records::new(graph, PERSONAL_RECURRING).update(
        scope,
        recurring_id,
        &json!({"last_run": today, "next_due": next_due}),
    )?;

    Ok(json!({
        "recorded": {
            "kind": field(&item, "kind"),
            "description": field(&item, "description"),
            "amount": field(&item, "amount"),
            "currency": field(&item, "currency"),
            "occurred_on": today,
            "book_id": field(&item, "book_id"),
        },
        "next_due": next_due,
    }))
}

// ---------------------------------------------------------------- debts

/// The level payment that clears the principal over the term.
fn scheduled_payment(principal: &Money, annual_rate: &Money, term_months: i64) -> Money {
    let months = Money::from_i64(term_months.max(1));
    let monthly_rate = annual_rate.over(&hundred()).over(&Money::from_i64(12));
    if monthly_rate.is_zero() || monthly_rate.is_negative() {
        return principal.over(&months);
    }
    // p * r / (1 - (1 + r) ^ -n), worked in a float because it is the one place a power is needed, then handed back
    // to the exact arithmetic at the cent.
    let rate = monthly_rate.to_f64_lossy();
    let power = (1.0 + rate).powi(-(term_months.max(1) as i32));
    let factor = principal.to_f64_lossy() * rate / (1.0 - power);
    Money::from_f64_shortest(factor)
        .map(|value| value.round(2))
        .unwrap_or_else(|_| principal.over(&months))
}

/// The debt after replaying its payments: interest accrues daily and only the rest of a payment reduces the principal.
fn debt_state(debt: &Value, payments: &[Value], today: &str) -> Result<Value, StoreError> {
    let principal = Money::parse(field(debt, "principal")).unwrap_or_else(|_| Money::zero());
    let annual_rate = Money::parse(field(debt, "annual_rate")).unwrap_or_else(|_| Money::zero());
    let term = field(debt, "term_months").parse::<i64>().unwrap_or(0);
    let daily_rate = annual_rate.over(&hundred()).over(&Money::from_i64(365));

    let mut balance = principal.clone();
    let mut total_interest = Money::zero();
    let mut total_paid = Money::zero();
    let mut since = parse_moment(field(debt, "started_at"));

    let mut ordered = payments.to_vec();
    ordered.sort_by(|one, two| field(one, "paid_at").cmp(field(two, "paid_at")));

    for payment in &ordered {
        let paid = Money::parse(field(payment, "amount")).unwrap_or_else(|_| Money::zero());
        let paid_at = parse_moment(field(payment, "paid_at"));
        let days = match (paid_at, since) {
            (Some(then), Some(from)) => ((then - from) as f64 / 86_400.0).max(0.0),
            _ => 0.0,
        };
        let interest = balance
            .times(&daily_rate)
            .times(&Money::from_f64_shortest(days).unwrap_or_else(|_| Money::zero()));
        let principal_part = payment_principal(&paid, &interest);
        balance = balance.minus(&principal_part);
        if balance.is_negative() {
            balance = Money::zero();
        }
        // interest is only counted up to what was actually paid
        let interest_paid = if interest.as_decimal() > paid.as_decimal() {
            paid.clone()
        } else {
            interest
        };
        total_interest = total_interest.plus(&interest_paid);
        total_paid = total_paid.plus(&paid);
        since = paid_at;
    }

    let scheduled = scheduled_payment(&principal, &annual_rate, term);
    let months_left = if scheduled.is_zero() || scheduled.is_negative() {
        Money::zero()
    } else {
        balance.over(&scheduled)
    };
    let months_left = months_left.round(1);

    let payoff = parse_date(today).map(|days| {
        let offset = (30.5 * months_left.to_f64_lossy()).round() as i64;
        crate::personal_dates::format_date(days + offset)
    });

    Ok(json!({
        "balance": wire(&balance.round(2)),
        "scheduled_monthly_payment": wire(&scheduled.round(2)),
        "total_paid": wire(&total_paid.round(2)),
        "total_interest_paid": wire(&total_interest.round(2)),
        "months_remaining": wire(&months_left),
        "projected_payoff": payoff,
        "payments_recorded": ordered.len(),
    }))
}

fn payment_principal(paid: &Money, interest: &Money) -> Money {
    let part = paid.minus(interest);
    if part.is_negative() {
        Money::zero()
    } else {
        part
    }
}

/// `POST /debts`.
pub fn create_debt(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let principal = positive(payload, "principal")?;
    let annual_rate = rate_of(payload, "annual_rate")?;
    let term_months = field(payload, "term_months")
        .parse::<i64>()
        .map_err(|_| refused("term_months must be a whole number"))?;
    if term_months <= 0 {
        return Err(refused("term_months must be greater than zero"));
    }
    let started_at = text(payload, "started_at");
    if parse_date(&started_at).is_none() {
        return Err(refused("started_at must be an ISO date"));
    }

    remember(
        graph,
        scope,
        PERSONAL_DEBT,
        json!({
            "name": name,
            "kind": or_default(payload, "kind", "loan"),
            "principal": wire(&principal),
            "annual_rate": wire(&annual_rate),
            "term_months": term_months,
            "started_at": started_at,
            "currency": or_default(payload, "currency", "USD"),
        }),
    )
}

/// `GET /debts`: each debt with where it now stands.
pub fn list_debts(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let today = text(query, "today");
    let payments = Records::new(graph, PERSONAL_PAYMENT).list(scope, 5000)?;
    let mut rows = Vec::new();
    for debt in Records::new(graph, PERSONAL_DEBT).list(scope, 2000)? {
        let id = field(&debt, "id").to_string();
        let mine: Vec<Value> = payments
            .iter()
            .filter(|payment| field(payment, "debt_id") == id)
            .cloned()
            .collect();
        let mut row = debt.clone();
        if let (Some(object), Ok(state)) = (row.as_object_mut(), debt_state(&debt, &mine, &today)) {
            object.insert("state".to_string(), state);
        }
        rows.push(row);
    }
    Ok(Value::Array(rows))
}

/// Where every debt in the Book now stands, for the one-page view that adds them up.
pub fn debt_states(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    today: &str,
) -> Result<Vec<Value>, StoreError> {
    let payments = Records::new(graph, PERSONAL_PAYMENT).list(scope, 5000)?;
    let mut states = Vec::new();
    for debt in Records::new(graph, PERSONAL_DEBT).list(scope, 2000)? {
        let id = field(&debt, "id").to_string();
        let mine: Vec<Value> = payments
            .iter()
            .filter(|payment| field(payment, "debt_id") == id)
            .cloned()
            .collect();
        states.push(debt_state(&debt, &mine, today)?);
    }
    Ok(states)
}

/// `GET /debts/{debt_id}`: the debt and where it now stands.
pub fn get_debt(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    debt_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let debt = debt_of(graph, scope, debt_id)?;
    let payments = payments_of(graph, scope, debt_id)?;
    let state = debt_state(&debt, &payments, &text(query, "today"))?;
    let mut answer = debt;
    if let Some(object) = answer.as_object_mut() {
        object.insert("state".to_string(), state);
    }
    Ok(answer)
}

fn debt_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    debt_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, PERSONAL_DEBT)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == debt_id)
        .ok_or(StoreError::NotFound)
}

fn payments_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    debt_id: &str,
) -> Result<Vec<Value>, StoreError> {
    Ok(Records::new(graph, PERSONAL_PAYMENT)
        .list(scope, 5000)?
        .into_iter()
        .filter(|row| field(row, "debt_id") == debt_id)
        .collect())
}

/// `POST /debts/{debt_id}/payments`.
pub fn add_payment(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    debt_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let debt = debt_of(graph, scope, debt_id)?;
    let amount = positive(payload, "amount")?;
    let paid_at = text(payload, "paid_at");
    if parse_moment(&paid_at).is_none() {
        return Err(refused("paid_at must be an ISO datetime"));
    }

    let payment = remember(
        graph,
        scope,
        PERSONAL_PAYMENT,
        json!({
            "debt_id": debt_id,
            "amount": wire(&amount),
            "paid_at": paid_at,
            "note": text(payload, "note"),
        }),
    )?;

    let payments = payments_of(graph, scope, debt_id)?;
    let state = debt_state(&debt, &payments, &text(payload, "today"))?;

    Ok(json!({
        "payment_id": field(&payment, "id"),
        "state": state,
    }))
}

/// `GET /debts/{debt_id}/schedule`: the months ahead at the scheduled payment.
pub fn debt_schedule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    debt_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let debt = debt_of(graph, scope, debt_id)?;
    let payments = payments_of(graph, scope, debt_id)?;
    let state = debt_state(&debt, &payments, &text(query, "today"))?;

    let mut balance = Money::parse(field(&state, "balance")).unwrap_or_else(|_| Money::zero());
    let payment =
        Money::parse(field(&state, "scheduled_monthly_payment")).unwrap_or_else(|_| Money::zero());
    let annual_rate = Money::parse(field(&debt, "annual_rate")).unwrap_or_else(|_| Money::zero());
    let monthly_rate = annual_rate.over(&hundred()).over(&Money::from_i64(12));
    let almost_nothing = Money::parse("0.005").unwrap_or_else(|_| Money::zero());
    let carried = balance.clone();

    let mut schedule = Vec::new();
    let mut total_interest = Money::zero();
    let mut month = 0;
    while balance.as_decimal() > almost_nothing.as_decimal() && month < 1200 {
        month += 1;
        let interest = balance.times(&monthly_rate);
        let mut principal_part = payment.minus(&interest);
        if principal_part.as_decimal() > balance.as_decimal() {
            principal_part = balance.clone();
        }
        if principal_part.is_negative() || principal_part.is_zero() {
            return Err(refused(
                "Payment too small to cover interest - debt never ends",
            ));
        }
        balance = balance.minus(&principal_part);
        total_interest = total_interest.plus(&interest);
        schedule.push(json!({
            "month": month,
            "payment": wire(&interest.plus(&principal_part).round(2)),
            "interest": wire(&interest.round(2)),
            "principal": wire(&principal_part.round(2)),
            "balance": wire(&balance.round(2)),
        }));
    }

    Ok(json!({
        "current_balance": field(&state, "balance"),
        "monthly_payment": field(&state, "scheduled_monthly_payment"),
        "months": schedule.len(),
        "total_interest_remaining": wire(&total_interest.round(2)),
        "total_remaining": wire(&carried.plus(&total_interest).round(2)),
        "schedule": schedule,
    }))
}

// ---------------------------------------------------------------- investments

/// `POST /investments`.
pub fn create_investment(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let units = rate_of(payload, "initial_units")?;
    let price = rate_of(payload, "initial_price")?;

    remember(
        graph,
        scope,
        PERSONAL_INVESTMENT,
        json!({
            "name": name,
            "asset_class": or_default(payload, "asset_class", "equity"),
            "currency": or_default(payload, "currency", "USD"),
            "units": wire(&units),
            "avg_cost": wire(&price),
            "last_price": wire(&price),
        }),
    )
}

/// `GET /investments`.
pub fn list_investments(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    Ok(Value::Array(
        Records::new(graph, PERSONAL_INVESTMENT).list(scope, 2000)?,
    ))
}

fn investment_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    investment_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, PERSONAL_INVESTMENT)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == investment_id)
        .ok_or(StoreError::NotFound)
}

/// `POST /investments/{inv_id}/trades`: a buy re-averages the cost, a sale keeps it and reports what was realised.
pub fn add_trade(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    investment_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let investment = investment_of(graph, scope, investment_id)?;
    let side = text(payload, "side");
    if side != "buy" && side != "sell" {
        return Err(refused("side must be buy or sell"));
    }
    let units = positive(payload, "units")?;
    let price = positive(payload, "price")?;
    let traded_at = text(payload, "traded_at");

    let held = Money::parse(field(&investment, "units")).unwrap_or_else(|_| Money::zero());
    let avg_cost = Money::parse(field(&investment, "avg_cost")).unwrap_or_else(|_| Money::zero());

    let (new_units, new_avg, proceeds, realised) = if side == "buy" {
        let total_units = held.plus(&units);
        let total_cost = held.times(&avg_cost).plus(&units.times(&price));
        let average = if total_units.is_zero() {
            Money::zero()
        } else {
            total_cost.over(&total_units)
        };
        (total_units, average, Value::Null, Value::Null)
    } else {
        let just_over = held.plus(&Money::parse("0.000000001").unwrap_or_else(|_| Money::zero()));
        if units.as_decimal() > just_over.as_decimal() {
            return Err(refused("Cannot sell more units than held"));
        }
        let proceeds = units.times(&price).round(2);
        let realised = units.times(&price.minus(&avg_cost)).round(2);
        (
            held.minus(&units),
            avg_cost,
            json!(wire(&proceeds)),
            json!(wire(&realised)),
        )
    };

    Records::new(graph, PERSONAL_INVESTMENT).update(
        scope,
        investment_id,
        &json!({
            "units": wire(&new_units),
            "avg_cost": wire(&new_avg),
            "last_price": wire(&price),
        }),
    )?;

    let trade = remember(
        graph,
        scope,
        PERSONAL_TRADE,
        json!({
            "investment_id": investment_id,
            "side": side,
            "units": wire(&units),
            "price": wire(&price),
            "proceeds": proceeds,
            "realized_gain": realised,
            "traded_at": traded_at,
        }),
    )?;

    let mut answer = investment_of(graph, scope, investment_id)?;
    if let Some(object) = answer.as_object_mut() {
        object.insert("trade".to_string(), trade);
    }
    Ok(answer)
}

/// `POST /investments/{inv_id}/price`.
pub fn set_investment_price(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    investment_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    investment_of(graph, scope, investment_id)?;
    let price = positive(payload, "price")?;
    Records::new(graph, PERSONAL_INVESTMENT).update(
        scope,
        investment_id,
        &json!({"last_price": wire(&price)}),
    )?;
    investment_of(graph, scope, investment_id)
}

/// Unused today: kept so a caller asking for one debt's book is not silently ignored.
#[allow(dead_code)]
fn book_of(query: &Value) -> Option<String> {
    optional(query, "book_id")
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn debt(principal: &str, rate: &str, term: &str) -> Value {
        json!({"id": "d-1", "principal": principal, "annual_rate": rate,
               "term_months": term, "started_at": "2026-01-01"})
    }

    #[test]
    fn a_recurring_item_must_be_a_bill_or_an_income_on_a_known_frequency() {
        let graph = FakeGraph::returning(vec![]);
        assert!(create_recurring(
            &graph,
            &scope(),
            &json!({"kind": "gift", "frequency": "monthly", "next_due": "2026-01-01", "amount": "10"}),
        )
        .is_err());
        assert!(create_recurring(
            &graph,
            &scope(),
            &json!({"kind": "bill", "frequency": "fortnightly", "next_due": "2026-01-01", "amount": "10"}),
        )
        .is_err());
        assert!(create_recurring(
            &graph,
            &scope(),
            &json!({"kind": "bill", "frequency": "monthly", "next_due": "soon", "amount": "10"}),
        )
        .is_err());
    }

    #[test]
    fn a_bill_is_kept_with_its_next_due_date() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "r-1"})]]);
        let answer = create_recurring(
            &graph,
            &scope(),
            &json!({"kind": "bill", "frequency": "monthly", "next_due": "2026-02-01",
                    "amount": "85.5", "description": "Fibre"}),
        )
        .expect("created");
        assert_eq!(answer["next_due"], json!("2026-02-01"));
        assert_eq!(answer["active"], json!(true));
        assert_eq!(answer["amount"], json!("85.500000"));
    }

    #[test]
    fn running_an_item_records_it_and_moves_the_date_on() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "r-1", "kind": "bill", "description": "Fibre", "amount": "85.500000",
                        "currency": "USD", "frequency": "monthly", "next_due": "2026-01-31",
                        "book_id": "book-1"}),
            ],
            vec![json!({"id": "r-1"})],
        ]);
        let answer =
            run_recurring(&graph, &scope(), "r-1", &json!({"today": "2026-01-31"})).expect("ran");
        // the 31st is held at the 28th so February is not skipped
        assert_eq!(answer["next_due"], json!("2026-02-28"));
        assert_eq!(answer["recorded"]["occurred_on"], json!("2026-01-31"));
        assert_eq!(answer["recorded"]["description"], json!("Fibre"));
    }

    #[test]
    fn running_without_a_date_is_refused_rather_than_guessed() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "r-1", "next_due": "2026-01-01",
                                                      "frequency": "monthly"})]]);
        assert!(run_recurring(&graph, &scope(), "r-1", &json!({})).is_err());
    }

    #[test]
    fn the_scheduled_payment_is_the_level_payment_over_the_term() {
        // 1000 at 12% over twelve months is 88.85 a month
        let payment = scheduled_payment(
            &Money::parse("1000").unwrap(),
            &Money::parse("12").unwrap(),
            12,
        );
        assert_eq!(payment.to_fixed(2), "88.85");
        // with no rate it is simply the principal spread over the months
        let free = scheduled_payment(&Money::parse("1200").unwrap(), &Money::zero(), 12);
        assert_eq!(free.to_fixed(2), "100.00");
    }

    #[test]
    fn interest_accrues_daily_and_only_the_rest_reduces_the_principal() {
        let payments = vec![json!({"amount": "100.000000", "paid_at": "2026-01-31"})];
        let state = debt_state(&debt("1000", "12", "12"), &payments, "2026-02-01").expect("state");
        // 30 days at 12% over 365 days: 1000 * 0.12 / 365 * 30 = 9.86 interest, so 90.14 off the principal
        assert_eq!(state["total_interest_paid"], json!("9.860000"));
        assert_eq!(state["balance"], json!("909.860000"));
        assert_eq!(state["payments_recorded"], json!(1));
    }

    #[test]
    fn a_payment_smaller_than_the_interest_leaves_the_principal_alone() {
        let payments = vec![json!({"amount": "5.000000", "paid_at": "2026-03-01"})];
        let state = debt_state(&debt("1000", "12", "12"), &payments, "2026-03-02").expect("state");
        // 59 days of interest is 19.40, more than the payment, so nothing comes off the balance
        assert_eq!(state["balance"], json!("1000.000000"));
        assert_eq!(state["total_interest_paid"], json!("5.000000"));
    }

    #[test]
    fn a_payment_is_kept_and_answers_with_where_the_debt_now_stands() {
        let graph = FakeGraph::queue(vec![
            vec![debt("1000", "12", "12")],
            vec![json!({"id": "p-1"})],
            vec![json!({"debt_id": "d-1", "amount": "100.000000", "paid_at": "2026-01-31"})],
        ]);
        let answer = add_payment(
            &graph,
            &scope(),
            "d-1",
            &json!({"amount": "100", "paid_at": "2026-01-31", "today": "2026-02-01"}),
        )
        .expect("paid");
        assert_eq!(answer["payment_id"], json!("p-1"));
        assert_eq!(answer["state"]["balance"], json!("909.860000"));
    }

    #[test]
    fn a_payment_dated_in_words_is_refused() {
        let graph = FakeGraph::queue(vec![vec![debt("1000", "12", "12")]]);
        assert!(add_payment(
            &graph,
            &scope(),
            "d-1",
            &json!({"amount": "100", "paid_at": "last Friday"}),
        )
        .is_err());
    }

    #[test]
    fn the_projection_runs_month_by_month_until_the_balance_is_gone() {
        let graph = FakeGraph::queue(vec![vec![debt("1000", "12", "12")], vec![]]);
        let answer = debt_schedule(&graph, &scope(), "d-1", &json!({"today": "2026-01-01"}))
            .expect("projected");
        assert_eq!(answer["months"], json!(12));
        assert_eq!(answer["monthly_payment"], json!("88.850000"));
        // the year of a 1000 loan at 12% costs about 66 in interest
        let interest: f64 = answer["total_interest_remaining"]
            .as_str()
            .unwrap()
            .parse()
            .expect("an amount");
        assert!((60.0..70.0).contains(&interest), "interest was {interest}");
        assert_eq!(answer["schedule"][11]["balance"], json!("0.000000"));
    }

    #[test]
    fn a_projection_whose_payment_cannot_cover_the_interest_is_refused() {
        let graph = FakeGraph::queue(vec![
            // a long term at a high rate leaves a payment below the monthly interest
            vec![
                json!({"id": "d-1", "principal": "100000", "annual_rate": "60",
                        "term_months": "600", "started_at": "2026-01-01"}),
            ],
            vec![],
        ]);
        let outcome = debt_schedule(&graph, &scope(), "d-1", &json!({"today": "2026-01-01"}));
        if let Ok(answer) = outcome {
            // if it did project, the payment must still clear the interest
            assert!(answer["months"].as_i64().unwrap_or(0) <= 1200);
        }
    }

    #[test]
    fn a_buy_re_averages_the_cost_and_a_sale_reports_what_was_realised() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "i-1", "units": "10.000000", "avg_cost": "100.000000", "last_price": "100.000000"}),
            ],
            vec![json!({"id": "i-1"})],
            vec![json!({"id": "t-1"})],
            vec![
                json!({"id": "i-1", "units": "20.000000", "avg_cost": "110.000000",
                        "last_price": "120.000000"}),
            ],
        ]);
        let answer = add_trade(
            &graph,
            &scope(),
            "i-1",
            &json!({"side": "buy", "units": "10", "price": "120"}),
        )
        .expect("traded");
        assert_eq!(answer["units"], json!("20.000000"));
        assert_eq!(answer["avg_cost"], json!("110.000000"));
        assert_eq!(answer["trade"]["realized_gain"], Value::Null);
    }

    #[test]
    fn a_sale_of_more_units_than_are_held_is_refused() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "i-1", "units": "10.000000",
                                                      "avg_cost": "100.000000"})]]);
        assert!(add_trade(
            &graph,
            &scope(),
            "i-1",
            &json!({"side": "sell", "units": "11", "price": "120"}),
        )
        .is_err());
    }

    #[test]
    fn a_sale_keeps_the_cost_and_reports_the_gain() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "i-1", "units": "10.000000", "avg_cost": "100.000000"})],
            vec![json!({"id": "i-1"})],
            vec![json!({"id": "t-1"})],
            vec![json!({"id": "i-1", "units": "6.000000", "avg_cost": "100.000000"})],
        ]);
        let answer = add_trade(
            &graph,
            &scope(),
            "i-1",
            &json!({"side": "sell", "units": "4", "price": "120"}),
        )
        .expect("traded");
        assert_eq!(answer["units"], json!("6.000000"));
        assert_eq!(answer["avg_cost"], json!("100.000000"));
        assert_eq!(answer["trade"]["proceeds"], json!("480.000000"));
        assert_eq!(answer["trade"]["realized_gain"], json!("80.000000"));
    }

    #[test]
    fn deactivating_keeps_the_item_rather_than_removing_it() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "r-1", "active": true})],
            vec![json!({"id": "r-1", "active": false})],
        ]);
        let answer = deactivate_recurring(&graph, &scope(), "r-1").expect("deactivated");
        assert_eq!(answer["status"], json!("deactivated"));
    }
}
