#![forbid(unsafe_code)]
//! tax-accounting-service: rates and registration, the tax on a transaction, returns, withholding and the
//! reports a tax manager reads.
//!
//! Nothing here moves money. A calculation stores the transaction it worked out and reports the journal it
//! would post; collecting or paying tax stays the accounting books' business, exactly as the shipped service
//! said in its own comments.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a calculation with no registered rate falls back to the default table rather than refusing;
//! * the rate used is the first registered rate that covers the day, not the newest;
//! * a return is measured from the transactions in its period and its net due is collected less paid plus
//!   whatever adjustment the caller stated - the figure is worked out, never accepted from the caller;
//! * the summary and the VAT breakdown ignore reversed transactions, because a reversal already has its own
//!   entry and counting both would double the period;
//! * the VAT breakdown puts every transaction that is not a sale on the paid side, which is what the shipped
//!   service did with imports, expenses and the rest;
//! * the liability schedule counts only a filed return as outstanding: a draft is not yet owed.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::read::{field, money, money_or_nothing, optional, or_default, text};
use vimbai_store::write::remember;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, DEFERRED_TAX, TAX_RATE, TAX_REGISTRATION, TAX_REPORT,
    TAX_RETURN, TAX_TRANSACTION, WITHHOLDING_ENTRY,
};

use crate::rates::{applicable, percent_for, split};

const LISTED: usize = 2000;

fn rows<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    kind: vimbai_store::RecordKind,
) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, kind).list(scope, LISTED)
}

/// The day a calculation is placed on: the caller's date, or today when none was given.
fn day_of(payload: &Value) -> String {
    let given = optional(payload, "transaction_date")
        .or_else(|| optional(payload, "date"))
        .unwrap_or_default();
    if given.is_empty() {
        text(payload, "today")
    } else {
        given
    }
}

fn date_part(value: &str) -> &str {
    value.get(..10).unwrap_or(value)
}

// --- rates ---

pub fn create_rate<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let tax_type = text(payload, "tax_type");
    if tax_type.is_empty() {
        return Err(StoreError::Invalid("tax_type is required".into()));
    }
    let jurisdiction = text(payload, "jurisdiction");
    if jurisdiction.is_empty() {
        return Err(StoreError::Invalid("jurisdiction is required".into()));
    }
    let mut body = payload.clone();
    if body.get("is_active").is_none() {
        body["is_active"] = json!(true);
    }
    memory(&mut body, "rate_percentage", "0");
    remember(graph, scope, TAX_RATE, body)
}

/// Fill a field from the request when it is absent, so the answer echoes what was sent.
fn memory(body: &mut Value, key: &str, fallback: &str) {
    if body.get(key).is_none() {
        body[key] = json!(fallback);
    }
}

pub fn list_rates<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let tax_type = optional(query, "tax_type").unwrap_or_default();
    let jurisdiction = optional(query, "jurisdiction").unwrap_or_default();
    let way = query.get("is_active").and_then(Value::as_bool);
    let rates: Vec<Value> = rows(graph, scope, TAX_RATE)?
        .into_iter()
        .filter(|rate| {
            (tax_type.is_empty() || field(rate, "tax_type") == tax_type)
                && (jurisdiction.is_empty() || field(rate, "jurisdiction") == jurisdiction)
                && way
                    .map(|wanted| {
                        rate.get("is_active")
                            .and_then(Value::as_bool)
                            .unwrap_or(true)
                            == wanted
                    })
                    .unwrap_or(true)
        })
        .collect();
    Ok(json!({ "rates": rates, "count": rates.len() }))
}

pub fn get_rate<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    rate_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, TAX_RATE).get(scope, rate_id)
}

pub fn update_rate<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    rate_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let mut patch = payload.clone();
    if let Some(fields) = patch.as_object_mut() {
        // the record's own identity is never a field being edited
        fields.remove("id");
        fields.remove("user_id");
        fields.remove("book_id");
    }
    Records::new(graph, TAX_RATE).update(scope, rate_id, &patch)
}

pub fn create_registration<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let tax_type = text(payload, "tax_type");
    if tax_type.is_empty() {
        return Err(StoreError::Invalid("tax_type is required".into()));
    }
    let number = text(payload, "registration_number");
    if number.is_empty() {
        return Err(StoreError::Invalid(
            "registration_number is required".into(),
        ));
    }
    let mut body = payload.clone();
    if body.get("is_active").is_none() {
        body["is_active"] = json!(true);
    }
    remember(graph, scope, TAX_REGISTRATION, body)
}

pub fn list_registrations<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let tax_type = optional(query, "tax_type").unwrap_or_default();
    let jurisdiction = optional(query, "jurisdiction").unwrap_or_default();
    let way = query.get("is_active").and_then(Value::as_bool);
    let registrations: Vec<Value> = rows(graph, scope, TAX_REGISTRATION)?
        .into_iter()
        .filter(|row| {
            (tax_type.is_empty() || field(row, "tax_type") == tax_type)
                && (jurisdiction.is_empty() || field(row, "jurisdiction") == jurisdiction)
                && way
                    .map(|wanted| {
                        row.get("is_active")
                            .and_then(Value::as_bool)
                            .unwrap_or(true)
                            == wanted
                    })
                    .unwrap_or(true)
        })
        .collect();
    Ok(json!({ "registrations": registrations, "count": registrations.len() }))
}

// --- calculation ---

/// The rate that will be used for this call, and the type it was filed under.
fn rate_used<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    tax_type: &str,
    jurisdiction: &str,
    day: &str,
) -> Result<(Money, String), StoreError> {
    let registered = rows(graph, scope, TAX_RATE)?;
    let found = applicable(&registered, tax_type, jurisdiction, day);
    let percent = percent_for(found.as_ref(), tax_type)?;
    let rate_type = found
        .as_ref()
        .map(|row| or_default(row, "rate_type", "standard"))
        .unwrap_or_else(|| "standard".to_owned());
    Ok((percent, rate_type))
}

pub fn calculate<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let tax_type = text(payload, "tax_type");
    if tax_type.is_empty() {
        return Err(StoreError::Invalid("tax_type is required".into()));
    }
    let jurisdiction = text(payload, "jurisdiction");
    let kind = or_default(payload, "transaction_type", "sale");
    let day = day_of(payload);
    let is_net = payload
        .get("is_net")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let amount = money(payload, "gross_amount")?;
    let (percent, rate_type) = rate_used(graph, scope, &tax_type, &jurisdiction, date_part(&day))?;
    let worked = split(&amount, &percent, is_net);

    let body = json!({
        "transaction_date": day,
        "transaction_type": kind,
        "tax_type": tax_type,
        "jurisdiction": jurisdiction,
        "gross_amount": wire(&worked.gross),
        "net_amount": wire(&worked.net),
        "tax_amount": wire(&worked.tax),
        "rate_used": wire(&percent),
        "rate_type": rate_type,
        "is_reversed": false,
        "description": format!(
            "Tax calculation for {kind} in {jurisdiction}"
        ),
    });
    remember(graph, scope, TAX_TRANSACTION, body)
}

pub fn calculate_batch<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let items = match payload.as_array() {
        Some(items) => items.clone(),
        None => match payload.get("transactions").and_then(Value::as_array) {
            Some(items) => items.clone(),
            None => {
                return Err(StoreError::Invalid(
                    "a batch is a list of transactions".into(),
                ))
            }
        },
    };
    let mut answers = Vec::with_capacity(items.len());
    for item in &items {
        answers.push(calculate(graph, scope, item)?);
    }
    Ok(Value::Array(answers))
}

// --- returns ---

/// The transactions a return is measured on: the same tax type, jurisdiction and period, never a reversal.
fn in_period<'a>(rows: &'a [Value], body: &Value) -> Vec<&'a Value> {
    let tax_type = or_default(body, "tax_type", "");
    let jurisdiction = or_default(body, "jurisdiction", "");
    let start = date_part(&or_default(body, "period_start", "")).to_owned();
    let end = date_part(&or_default(body, "period_end", "")).to_owned();
    rows.iter()
        .filter(|row| {
            let on = date_part(field(row, "transaction_date")).to_owned();
            field(row, "tax_type") == tax_type
                && field(row, "jurisdiction") == jurisdiction
                && (start.is_empty() || on.as_str() >= start.as_str())
                && (end.is_empty() || on.as_str() <= end.as_str())
                && !row
                    .get("is_reversed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
        .collect()
}

fn total(rows: &[&Value], key: &str) -> Money {
    rows.iter().fold(Money::zero(), |running, row| {
        running.plus(&stored_amount(row, key).unwrap_or_else(|_| Money::zero()))
    })
}

pub fn create_return<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let tax_type = text(payload, "tax_type");
    if tax_type.is_empty() {
        return Err(StoreError::Invalid("tax_type is required".into()));
    }
    let all = rows(graph, scope, TAX_TRANSACTION)?;
    let period = in_period(&all, payload);
    let sales: Vec<&Value> = period
        .iter()
        .copied()
        .filter(|row| field(row, "transaction_type") == "sale")
        .collect();
    let purchases: Vec<&Value> = period
        .iter()
        .copied()
        .filter(|row| field(row, "transaction_type") == "purchase")
        .collect();

    let collected = total(&sales, "tax_amount");
    let paid = total(&purchases, "tax_amount");
    let adjustments = money_or_nothing(payload, "tax_adjustments")?;

    let mut body = payload.clone();
    body["gross_sales"] = wire(&total(&sales, "gross_amount"));
    body["gross_purchases"] = wire(&total(&purchases, "gross_amount"));
    body["tax_collected"] = wire(&collected);
    body["tax_paid"] = wire(&paid);
    // collected less paid, then whatever the caller stated as an adjustment
    body["net_tax_due"] = wire(&collected.minus(&paid).plus(&adjustments));
    if body.get("status").is_none() {
        body["status"] = json!("draft");
    }
    remember(graph, scope, TAX_RETURN, body)
}

pub fn list_returns<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let tax_type = optional(query, "tax_type").unwrap_or_default();
    let jurisdiction = optional(query, "jurisdiction").unwrap_or_default();
    let status = optional(query, "status").unwrap_or_default();
    let from = optional(query, "period_start")
        .map(|value| date_part(&value).to_owned())
        .unwrap_or_default();
    let returns: Vec<Value> = rows(graph, scope, TAX_RETURN)?
        .into_iter()
        .filter(|row| {
            (tax_type.is_empty() || field(row, "tax_type") == tax_type)
                && (jurisdiction.is_empty() || field(row, "jurisdiction") == jurisdiction)
                && (status.is_empty() || field(row, "status") == status)
                && (from.is_empty()
                    || date_part(field(row, "period_start")).to_owned().as_str() >= from.as_str())
        })
        .collect();
    Ok(json!({ "returns": returns, "count": returns.len() }))
}

pub fn file_return<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    return_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let today = text(payload, "today");
    Records::new(graph, TAX_RETURN).update(
        scope,
        return_id,
        &json!({ "status": "filed", "filed_date": today }),
    )
}

pub fn pay_return<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    return_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let today = text(payload, "today");
    Records::new(graph, TAX_RETURN).update(
        scope,
        return_id,
        &json!({ "status": "paid", "paid_date": today }),
    )
}

// --- withholding ---

pub fn record_withholding<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let recipient = text(payload, "recipient_name");
    if recipient.is_empty() {
        return Err(StoreError::Invalid("recipient_name is required".into()));
    }
    let gross = money(payload, "gross_amount")?;
    let rate = money_or_nothing(payload, "withholding_rate")?;
    // the tax withheld is the stated figure when one was given, otherwise the rate against the gross
    let stated = payload.get("withholding_amount");
    let withheld = match stated {
        Some(Value::Null) | None => crate::rates::split(&gross, &rate, true).tax,
        Some(_) => money(payload, "withholding_amount")?,
    };
    let mut body = payload.clone();
    body["withholding_amount"] = wire(&withheld);
    body["net_amount_paid"] = wire(&gross.minus(&withheld));
    remember(graph, scope, WITHHOLDING_ENTRY, body)
}

pub fn list_withholding<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let recipient = optional(query, "recipient_id").unwrap_or_default();
    let start = optional(query, "start_date")
        .map(|value| date_part(&value).to_owned())
        .unwrap_or_default();
    let end = optional(query, "end_date")
        .map(|value| date_part(&value).to_owned())
        .unwrap_or_default();
    let entries: Vec<Value> = rows(graph, scope, WITHHOLDING_ENTRY)?
        .into_iter()
        .filter(|row| {
            let on = date_part(field(row, "payment_date")).to_owned();
            (recipient.is_empty() || field(row, "recipient_id") == recipient)
                && (start.is_empty() || on.as_str() >= start.as_str())
                && (end.is_empty() || on.as_str() <= end.as_str())
        })
        .collect();
    Ok(json!({ "entries": entries, "count": entries.len() }))
}

// --- reports ---

pub fn summary_report<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let tax_type = text(payload, "tax_type");
    if tax_type.is_empty() {
        return Err(StoreError::Invalid("tax_type is required".into()));
    }
    let all = rows(graph, scope, TAX_TRANSACTION)?;
    let period = in_period(&all, payload);
    let sales: Vec<&Value> = period
        .iter()
        .copied()
        .filter(|row| field(row, "transaction_type") == "sale")
        .collect();
    let purchases: Vec<&Value> = period
        .iter()
        .copied()
        .filter(|row| field(row, "transaction_type") == "purchase")
        .collect();
    let collected = total(&sales, "tax_amount");
    let paid = total(&purchases, "tax_amount");

    let details: Vec<Value> = period
        .iter()
        .map(|row| {
            json!({
                "transaction_id": field(row, "id"),
                "date": field(row, "transaction_date"),
                "type": field(row, "transaction_type"),
                "gross": field(row, "gross_amount"),
                "tax": field(row, "tax_amount"),
            })
        })
        .collect();

    let report = json!({
        "report_type": "tax_summary",
        "tax_type": tax_type,
        "jurisdiction": or_default(payload, "jurisdiction", ""),
        "period_start": or_default(payload, "period_start", ""),
        "period_end": or_default(payload, "period_end", ""),
        "summary": {
            "total_transactions": period.len(),
            "total_sales": sales.len(),
            "total_purchases": purchases.len(),
            "gross_sales": wire(&total(&sales, "gross_amount")),
            "gross_purchases": wire(&total(&purchases, "gross_amount")),
            "tax_collected": wire(&collected),
            "tax_paid": wire(&paid),
            "net_tax_position": wire(&collected.minus(&paid)),
        },
        "details": details,
        "total_tax": wire(&total(&period, "tax_amount")),
    });
    remember(graph, scope, TAX_REPORT, report)
}

pub fn vat_by_jurisdiction<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let start = date_part(&or_default(payload, "period_start", "")).to_owned();
    let end = date_part(&or_default(payload, "period_end", "")).to_owned();
    let mut order: Vec<String> = Vec::new();
    let mut collected: Vec<Money> = Vec::new();
    let mut paid: Vec<Money> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();

    for row in rows(graph, scope, TAX_TRANSACTION)? {
        let tax_type = field(&row, "tax_type");
        if tax_type != "vat" && tax_type != "gst" {
            continue;
        }
        let on = date_part(field(&row, "transaction_date")).to_owned();
        if (!start.is_empty() && on.as_str() < start.as_str())
            || (!end.is_empty() && on.as_str() > end.as_str())
        {
            continue;
        }
        if row
            .get("is_reversed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let jurisdiction = field(&row, "jurisdiction").to_owned();
        let at = match order.iter().position(|seen| *seen == jurisdiction) {
            Some(at) => at,
            None => {
                order.push(jurisdiction);
                collected.push(Money::zero());
                paid.push(Money::zero());
                counts.push(0);
                order.len() - 1
            }
        };
        let tax = stored_amount(&row, "tax_amount").unwrap_or_else(|_| Money::zero());
        if field(&row, "transaction_type") == "sale" {
            collected[at] = collected[at].plus(&tax);
        } else {
            // everything that is not a sale is on the paid side, as shipped
            paid[at] = paid[at].plus(&tax);
        }
        counts[at] += 1;
    }

    let mut by_jurisdiction = serde_json::Map::new();
    for (at, name) in order.iter().enumerate() {
        by_jurisdiction.insert(
            name.clone(),
            json!({
                "collected": wire(&collected[at]),
                "paid": wire(&paid[at]),
                "count": counts[at],
            }),
        );
    }
    Ok(json!({
        "period": {
            "start": or_default(payload, "period_start", ""),
            "end": or_default(payload, "period_end", ""),
        },
        "by_jurisdiction": Value::Object(by_jurisdiction),
    }))
}

// --- deferred tax ---

pub fn deferred_tax<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let percent = money(payload, "tax_rate")?;
    let differences = payload
        .get("temporary_differences")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut asset = Money::zero();
    let mut liability = Money::zero();
    for difference in &differences {
        let amount = amount_or_zero(difference, "amount")?;
        let share = crate::rates::split(&amount, &percent, true).tax;
        if text(difference, "type") == "asset" {
            asset = asset.plus(&share);
        } else {
            liability = liability.plus(&share);
        }
    }
    let body = json!({
        "period_end": or_default(payload, "period_end", ""),
        "tax_rate": wire(&percent),
        "temporary_differences": differences,
        "deferred_tax_asset": wire(&asset),
        "deferred_tax_liability": wire(&liability),
    });
    remember(graph, scope, DEFERRED_TAX, body)
}

// --- liability schedule ---

pub fn liability_schedule<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let tax_type = optional(query, "tax_type").unwrap_or_default();
    let jurisdiction = optional(query, "jurisdiction").unwrap_or_default();
    let status = optional(query, "status").unwrap_or_default();
    let returns: Vec<Value> = rows(graph, scope, TAX_RETURN)?
        .into_iter()
        .filter(|row| field(row, "status") == "draft" || field(row, "status") == "filed")
        .filter(|row| {
            (tax_type.is_empty() || field(row, "tax_type") == tax_type)
                && (jurisdiction.is_empty() || field(row, "jurisdiction") == jurisdiction)
                && (status.is_empty() || field(row, "status") == status)
        })
        .collect();
    let outstanding = returns
        .iter()
        .filter(|row| field(row, "status") == "filed")
        .fold(Money::zero(), |running, row| {
            running.plus(&stored_amount(row, "net_tax_due").unwrap_or_else(|_| Money::zero()))
        });
    Ok(json!({
        "returns": returns,
        "total_outstanding": wire(&outstanding),
        "count": returns.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn money_of(value: &Value, key: &str) -> String {
        value[key].as_str().unwrap_or_default().to_owned()
    }

    #[test]
    fn a_sale_with_no_registered_rate_is_taxed_at_the_default() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "made-1"})]]);
        let answer = calculate(
            &graph,
            &scope(),
            &json!({
                "transaction_type": "sale",
                "tax_type": "vat",
                "jurisdiction": "ZW",
                "gross_amount": 120,
                "transaction_date": "2026-03-01",
            }),
        )
        .expect("calculated");
        assert_eq!(money_of(&answer, "net_amount"), "100.000000");
        assert_eq!(money_of(&answer, "tax_amount"), "20.000000");
        assert_eq!(money_of(&answer, "gross_amount"), "120.000000");
    }

    #[test]
    fn a_registered_rate_is_used_instead_of_the_default() {
        let registered = vec![json!({
            "id": "rate-1",
            "tax_type": "vat",
            "jurisdiction": "ZW",
            "rate_percentage": "15",
            "rate_type": "standard",
            "is_active": true,
            "effective_from": "2026-01-01T00:00:00",
        })];
        let graph = FakeGraph::queue(vec![registered, vec![json!({"id": "made-1"})]]);
        let answer = calculate(
            &graph,
            &scope(),
            &json!({
                "transaction_type": "sale",
                "tax_type": "vat",
                "jurisdiction": "ZW",
                "gross_amount": 115,
                "transaction_date": "2026-03-01",
            }),
        )
        .expect("calculated");
        assert_eq!(money_of(&answer, "tax_amount"), "15.000000");
        assert_eq!(money_of(&answer, "net_amount"), "100.000000");
    }

    #[test]
    fn a_net_amount_is_taxed_on_top() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "made-1"})]]);
        let answer = calculate(
            &graph,
            &scope(),
            &json!({
                "transaction_type": "sale",
                "tax_type": "vat",
                "jurisdiction": "ZW",
                "gross_amount": 100,
                "is_net": true,
                "transaction_date": "2026-03-01",
            }),
        )
        .expect("calculated");
        assert_eq!(money_of(&answer, "gross_amount"), "120.000000");
        assert_eq!(money_of(&answer, "tax_amount"), "20.000000");
    }

    #[test]
    fn a_return_is_measured_from_its_transactions() {
        let transactions = vec![
            json!({"id": "t1", "tax_type": "vat", "jurisdiction": "ZW", "transaction_type": "sale",
                   "transaction_date": "2026-03-05", "gross_amount": "120", "tax_amount": "20"}),
            json!({"id": "t2", "tax_type": "vat", "jurisdiction": "ZW", "transaction_type": "purchase",
                   "transaction_date": "2026-03-06", "gross_amount": "60", "tax_amount": "10"}),
            // outside the period, so not part of this return
            json!({"id": "t3", "tax_type": "vat", "jurisdiction": "ZW", "transaction_type": "sale",
                   "transaction_date": "2026-04-05", "gross_amount": "120", "tax_amount": "20"}),
            // a reversal is already accounted for by its counterpart
            json!({"id": "t4", "tax_type": "vat", "jurisdiction": "ZW", "transaction_type": "sale",
                   "transaction_date": "2026-03-07", "gross_amount": "120", "tax_amount": "20",
                   "is_reversed": true}),
        ];
        let graph = FakeGraph::queue(vec![transactions]);
        let answer = create_return(
            &graph,
            &scope(),
            &json!({
                "tax_type": "vat",
                "jurisdiction": "ZW",
                "period_start": "2026-03-01",
                "period_end": "2026-03-31",
            }),
        )
        .expect("return created");
        assert_eq!(money_of(&answer, "gross_sales"), "120.000000");
        assert_eq!(money_of(&answer, "tax_collected"), "20.000000");
        assert_eq!(money_of(&answer, "tax_paid"), "10.000000");
        assert_eq!(money_of(&answer, "net_tax_due"), "10.000000");
    }

    #[test]
    fn an_adjustment_is_added_to_what_is_due() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "made-1"})]]);
        let answer = create_return(
            &graph,
            &scope(),
            &json!({
                "tax_type": "vat",
                "jurisdiction": "ZW",
                "period_start": "2026-03-01",
                "period_end": "2026-03-31",
                "tax_adjustments": 7.5,
            }),
        )
        .expect("return created");
        assert_eq!(money_of(&answer, "net_tax_due"), "7.500000");
    }

    #[test]
    fn a_withheld_amount_is_worked_out_when_it_is_not_stated() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "made-1"})]]);
        let answer = record_withholding(
            &graph,
            &scope(),
            &json!({
                "recipient_id": "r-1",
                "recipient_name": "Kaguvi Consulting",
                "payment_date": "2026-03-01",
                "gross_amount": 200,
                "withholding_rate": 15,
            }),
        )
        .expect("recorded");
        assert_eq!(money_of(&answer, "withholding_amount"), "30.000000");
        assert_eq!(money_of(&answer, "net_amount_paid"), "170.000000");
    }

    #[test]
    fn a_stated_withheld_amount_is_kept_as_sent() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "made-1"})]]);
        let answer = record_withholding(
            &graph,
            &scope(),
            &json!({
                "recipient_id": "r-1",
                "recipient_name": "Kaguvi Consulting",
                "payment_date": "2026-03-01",
                "gross_amount": 200,
                "withholding_rate": 15,
                "withholding_amount": 25,
            }),
        )
        .expect("recorded");
        assert_eq!(money_of(&answer, "withholding_amount"), "25.000000");
        assert_eq!(money_of(&answer, "net_amount_paid"), "175.000000");
    }

    #[test]
    fn the_vat_breakdown_puts_everything_that_is_not_a_sale_on_the_paid_side() {
        let transactions = vec![
            json!({"tax_type": "vat", "jurisdiction": "ZW", "transaction_type": "sale",
                   "transaction_date": "2026-03-05", "tax_amount": "20"}),
            json!({"tax_type": "vat", "jurisdiction": "ZW", "transaction_type": "import",
                   "transaction_date": "2026-03-06", "tax_amount": "5"}),
            json!({"tax_type": "vat", "jurisdiction": "ZA", "transaction_type": "sale",
                   "transaction_date": "2026-03-07", "tax_amount": "15"}),
        ];
        let graph = FakeGraph::queue(vec![transactions]);
        let answer = vat_by_jurisdiction(
            &graph,
            &scope(),
            &json!({"period_start": "2026-03-01", "period_end": "2026-03-31"}),
        )
        .expect("breakdown");
        let zw = &answer["by_jurisdiction"]["ZW"];
        assert_eq!(zw["collected"], json!("20.000000"));
        assert_eq!(zw["paid"], json!("5.000000"));
        assert_eq!(zw["count"], json!(2));
        assert_eq!(
            answer["by_jurisdiction"]["ZA"]["collected"],
            json!("15.000000")
        );
    }

    #[test]
    fn deferred_tax_splits_assets_from_liabilities() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "made-1"})]]);
        let answer = deferred_tax(
            &graph,
            &scope(),
            &json!({
                "period_end": "2026-03-31",
                "tax_rate": 25,
                "temporary_differences": [
                    {"type": "asset", "amount": 400},
                    {"type": "liability", "amount": 200},
                ],
            }),
        )
        .expect("worked out");
        assert_eq!(money_of(&answer, "deferred_tax_asset"), "100.000000");
        assert_eq!(money_of(&answer, "deferred_tax_liability"), "50.000000");
    }

    #[test]
    fn only_a_filed_return_is_outstanding() {
        let returns = vec![
            json!({"id": "r1", "tax_type": "vat", "jurisdiction": "ZW", "status": "filed", "net_tax_due": "10"}),
            json!({"id": "r2", "tax_type": "vat", "jurisdiction": "ZW", "status": "draft", "net_tax_due": "99"}),
            json!({"id": "r3", "tax_type": "vat", "jurisdiction": "ZW", "status": "paid", "net_tax_due": "50"}),
        ];
        let graph = FakeGraph::queue(vec![returns]);
        let answer = liability_schedule(&graph, &scope(), &json!({})).expect("schedule");
        assert_eq!(money_of(&answer, "total_outstanding"), "10.000000");
        assert_eq!(answer["count"], json!(2));
    }

    #[test]
    fn a_batch_answers_once_per_transaction() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "made-1"})]]);
        let answer = calculate_batch(
            &graph,
            &scope(),
            &json!([
                {"transaction_type": "sale", "tax_type": "vat", "jurisdiction": "ZW",
                 "gross_amount": 120, "transaction_date": "2026-03-01"},
                {"transaction_type": "purchase", "tax_type": "vat", "jurisdiction": "ZW",
                 "gross_amount": 240, "transaction_date": "2026-03-02"},
            ]),
        )
        .expect("batch");
        let answers = answer.as_array().expect("a list");
        assert_eq!(answers.len(), 2);
        assert_eq!(money_of(&answers[1], "tax_amount"), "40.000000");
    }
}
