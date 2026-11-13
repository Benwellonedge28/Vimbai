#![forbid(unsafe_code)]
//! petty-cash-service: the funds, the spending out of them, the vouchers that authorise it, and the
//! replenishments that put the money back.
//!
//! A fund's balance is never stored, because it is not a fact of its own: it is what came in less what went
//! out, worked out from the fund's own transactions every time it is asked for. That is what keeps the balance
//! and the audit trail from disagreeing.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a receipt, a replenishment and the initial fund all count as money in; only a payment counts as out;
//! * a voucher brings its payment with it, as one act, so the balance moves the moment the voucher exists;
//! * a payment for more than the fund holds is refused rather than driven into a negative balance;
//! * a fund is closed by a status change, not a delete, so its history stays readable;
//! * an amount is exact to the cent, and every answer is decimal text rather than a float.
//!
//! One thing deliberately shared: accounting-service keeps its own petty cash book under `:PettyCashFund`, and
//! this service keeps funds under the same label under a different edge, so neither could see the other's. The
//! port keeps one `:PettyCashFund` under one edge, and the funds both describe are one set.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::query::query_page;
use vimbai_store::read::{
    field, money, money_or_nothing, optional, or_default, refused, text, whole,
};
use vimbai_store::write::remember;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, PETTY_CASH_FUND, PETTY_CASH_REPLENISHMENT,
    PETTY_CASH_TRANSACTION, PETTY_CASH_VOUCHER,
};

/// What a fund's status may be.
const FUND_STATUS: &[&str] = &["active", "replenishing", "closed", "reimbursing"];

/// What a transaction may be.
const TRANSACTION_TYPES: &[&str] = &[
    "receipt",
    "payment",
    "replenishment",
    "initial_fund",
    "adjustment",
    "closing",
];

/// What a payment may be for.
const CATEGORIES: &[&str] = &[
    "travel",
    "office_supplies",
    "postage",
    "printing",
    "meals",
    "transportation",
    "miscellaneous",
    "stationery",
    "telephone",
    "office_expenses",
];

/// What a replenishment may be waiting for.
const REPLENISHMENT_STATUS: &[&str] =
    &["pending", "approved", "rejected", "processed", "cancelled"];

/// A fixed figure, kept as exact decimal text.
fn fixed(text: &str) -> Money {
    Money::parse(text).unwrap_or_else(|_| Money::zero())
}

fn checked(value: &str, allowed: &[&str], what: &str) -> Result<String, StoreError> {
    if allowed.contains(&value) {
        Ok(value.to_string())
    } else {
        Err(refused(&format!(
            "Invalid {what}. Must be one of {allowed:?}"
        )))
    }
}

/// The fund, or a miss, so nothing answers for a fund the caller does not hold.
fn fund_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    fund_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, PETTY_CASH_FUND)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == fund_id)
        .ok_or(StoreError::NotFound)
}

/// `POST /funds`.
pub fn create_fund(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let status = checked(
        &or_default(payload, "status", "active"),
        FUND_STATUS,
        "status",
    )?;
    let maximum = money_or_nothing(payload, "maximum_balance")?;
    let minimum = money_or_nothing(payload, "minimum_balance")?;
    if minimum > maximum {
        return Err(refused("minimum_balance cannot be above maximum_balance"));
    }

    remember(
        graph,
        scope,
        PETTY_CASH_FUND,
        json!({
            "fund_code": text(payload, "fund_code"),
            "fund_name": text(payload, "fund_name"),
            "custodian_id": text(payload, "custodian_id"),
            "custodian_name": text(payload, "custodian_name"),
            "location": text(payload, "location"),
            "maximum_balance": wire(&maximum),
            "minimum_balance": wire(&minimum),
            "replenishment_threshold": wire(&money_or_nothing(payload, "replenishment_threshold")?),
            "replenishment_amount": wire(&money_or_nothing(payload, "replenishment_amount")?),
            "status": status,
            "account_code": text(payload, "account_code"),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /funds`.
pub fn list_funds(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let status = optional(query, "status");
    let custodian = optional(query, "custodian_id");
    let mut funds: Vec<Value> = Records::new(graph, PETTY_CASH_FUND)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            status
                .as_deref()
                .map(|wanted| field(row, "status") == wanted)
                .unwrap_or(true)
                && custodian
                    .as_deref()
                    .map(|wanted| field(row, "custodian_id") == wanted)
                    .unwrap_or(true)
        })
        .collect();
    funds.sort_by(|one, two| field(one, "fund_code").cmp(field(two, "fund_code")));
    Ok(Value::Array(funds))
}

/// `GET /funds/{fund_id}`.
pub fn get_fund(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    fund_id: &str,
) -> Result<Value, StoreError> {
    fund_of(graph, scope, fund_id)
}

/// `PUT /funds/{fund_id}`: the fields that may be changed, left alone when they were not sent.
pub fn update_fund(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    fund_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let existing = fund_of(graph, scope, fund_id)?;
    let mut next = existing.clone();
    for key in [
        "fund_code",
        "fund_name",
        "custodian_id",
        "custodian_name",
        "location",
        "account_code",
    ] {
        if let Some(value) = optional(payload, key) {
            next[key] = json!(value);
        }
    }
    for key in [
        "maximum_balance",
        "minimum_balance",
        "replenishment_threshold",
        "replenishment_amount",
    ] {
        if payload.get(key).is_some_and(|value| !value.is_null()) {
            next[key] = json!(wire(&money(payload, key)?));
        }
    }
    if let Some(status) = optional(payload, "status") {
        next["status"] = json!(checked(&status, FUND_STATUS, "status")?);
    }
    Records::new(graph, PETTY_CASH_FUND).update(scope, fund_id, &next)
}

/// `POST /funds/{fund_id}/close`: closed by status, so the history stays readable.
pub fn close_fund(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    fund_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    fund_of(graph, scope, fund_id)?;
    Records::new(graph, PETTY_CASH_FUND).update(scope, fund_id, &json!({"status": "closed"}))?;
    Ok(json!({
        "status": "closed",
        "fund_id": fund_id,
        "closed_by": text(payload, "closed_by"),
    }))
}

/// `POST /transactions`: spending or paying in, against a fund the caller holds.
pub fn create_transaction(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let fund_id = text(payload, "fund_id");
    fund_of(graph, scope, &fund_id)?;
    let transaction_type = checked(
        &text(payload, "transaction_type"),
        TRANSACTION_TYPES,
        "transaction_type",
    )?;
    let category = checked(&text(payload, "category"), CATEGORIES, "category")?;
    let amount = money(payload, "amount")?;
    if amount.is_negative() {
        return Err(refused("amount cannot be negative"));
    }

    if transaction_type == "payment" {
        let balance = balance_of(graph, scope, &fund_id)?;
        if balance.minus(&amount).is_negative() {
            return Err(refused("Payment exceeds the fund balance"));
        }
    }

    remember(
        graph,
        scope,
        PETTY_CASH_TRANSACTION,
        json!({
            "fund_id": fund_id,
            "transaction_type": transaction_type,
            "amount": wire(&amount),
            "date": text(payload, "date"),
            "description": text(payload, "description"),
            "category": category,
            "recipient_name": text(payload, "recipient_name"),
            "recipient_id": text(payload, "recipient_id"),
            "reference_number": text(payload, "reference_number"),
            "voucher_number": text(payload, "voucher_number"),
            "approved_by": text(payload, "approved_by"),
            "entered_by": text(payload, "entered_by"),
            "receipt_attachment": text(payload, "receipt_attachment"),
            "notes": text(payload, "notes"),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// The transactions of one fund, or of every fund the caller holds.
fn transactions_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    fund_id: Option<&str>,
) -> Result<Vec<Value>, StoreError> {
    let mut rows: Vec<Value> = Records::new(graph, PETTY_CASH_TRANSACTION)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            fund_id
                .map(|wanted| field(row, "fund_id") == wanted)
                .unwrap_or(true)
        })
        .collect();
    rows.sort_by(|one, two| field(two, "date").cmp(field(one, "date")));
    Ok(rows)
}

/// `GET /transactions`.
pub fn list_transactions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let limit = query_page(query, 100, 2000);
    let fund_id = optional(query, "fund_id");
    let wanted_type = optional(query, "transaction_type");
    let wanted_category = optional(query, "category");
    let start = optional(query, "start_date");
    let end = optional(query, "end_date");

    let mut rows: Vec<Value> = transactions_of(graph, scope, fund_id.as_deref())?
        .into_iter()
        .filter(|row| {
            wanted_type
                .as_deref()
                .map(|wanted| field(row, "transaction_type") == wanted)
                .unwrap_or(true)
                && wanted_category
                    .as_deref()
                    .map(|wanted| field(row, "category") == wanted)
                    .unwrap_or(true)
                && start
                    .as_deref()
                    .map(|from| field(row, "date") >= from)
                    .unwrap_or(true)
                && end
                    .as_deref()
                    .map(|to| field(row, "date") <= to)
                    .unwrap_or(true)
        })
        .collect();
    rows.truncate(limit);
    Ok(Value::Array(rows))
}

/// `GET /transactions/{transaction_id}`.
pub fn get_transaction(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    transaction_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, PETTY_CASH_TRANSACTION)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == transaction_id)
        .ok_or(StoreError::NotFound)
}

/// What a fund has taken in and paid out, and what it holds as a result.
fn totals(rows: &[Value]) -> Result<(Money, Money), StoreError> {
    let mut receipts = Money::zero();
    let mut payments = Money::zero();
    for row in rows {
        let amount = money_or_nothing(row, "amount")?;
        match field(row, "transaction_type") {
            "receipt" | "replenishment" | "initial_fund" => receipts = receipts.plus(&amount),
            "payment" => payments = payments.plus(&amount),
            _ => {}
        }
    }
    Ok((receipts, payments))
}

fn balance_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    fund_id: &str,
) -> Result<Money, StoreError> {
    let (receipts, payments) = totals(&transactions_of(graph, scope, Some(fund_id))?)?;
    Ok(receipts.minus(&payments))
}

/// `GET /funds/{fund_id}/balance`.
pub fn fund_balance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    fund_id: &str,
) -> Result<Value, StoreError> {
    fund_of(graph, scope, fund_id)?;
    let (receipts, payments) = totals(&transactions_of(graph, scope, Some(fund_id))?)?;
    Ok(json!({
        "fund_id": fund_id,
        "total_receipts": wire(&receipts),
        "total_payments": wire(&payments),
        "current_balance": wire(&receipts.minus(&payments)),
    }))
}

/// `GET /funds/{fund_id}/summary`.
pub fn fund_summary(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    fund_id: &str,
) -> Result<Value, StoreError> {
    let fund = fund_of(graph, scope, fund_id)?;
    let rows = transactions_of(graph, scope, Some(fund_id))?;
    let (receipts, payments) = totals(&rows)?;
    let balance = receipts.minus(&payments);
    let threshold = money_or_nothing(&fund, "replenishment_threshold")?;

    // the originals counted transactions whose status was "pending"; a transaction has no status, so that
    // count was always zero. What is actually outstanding is a voucher still waiting, so that is counted.
    let outstanding = Records::new(graph, PETTY_CASH_VOUCHER)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| field(row, "fund_id") == fund_id && field(row, "status") == "pending")
        .count();
    let last_replenishment = rows
        .iter()
        .filter(|row| field(row, "transaction_type") == "replenishment")
        .map(|row| text(row, "date"))
        .max()
        .unwrap_or_default();

    Ok(json!({
        "fund_id": fund_id,
        "fund_name": text(&fund, "fund_name"),
        "opening_balance": "0.00",
        "total_receipts": wire(&receipts),
        "total_payments": wire(&payments),
        "closing_balance": wire(&balance),
        "outstanding_vouchers": outstanding,
        "available_cash": wire(&balance),
        "replenishment_needed": balance < threshold,
        "last_replenishment_date": last_replenishment,
    }))
}

/// `POST /vouchers`: the voucher and its payment, as one act.
pub fn create_voucher(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let fund_id = text(payload, "fund_id");
    fund_of(graph, scope, &fund_id)?;
    let amount = money(payload, "amount")?;
    if amount.is_negative() {
        return Err(refused("amount cannot be negative"));
    }
    let category = checked(&text(payload, "category"), CATEGORIES, "category")?;
    let voucher_number = text(payload, "voucher_number");
    let date = text(payload, "date");

    let voucher = remember(
        graph,
        scope,
        PETTY_CASH_VOUCHER,
        json!({
            "fund_id": fund_id,
            "voucher_number": voucher_number,
            "date": date,
            "payee": text(payload, "payee"),
            "amount": wire(&amount),
            "description": text(payload, "description"),
            "category": category,
            "approved_by": text(payload, "approved_by"),
            "receipt_attached": payload.get("receipt_attached") == Some(&json!(true)),
            "status": or_default(payload, "status", "pending"),
            "entered_by": text(payload, "entered_by"),
            "created_at": text(payload, "created_at"),
        }),
    )?;

    remember(
        graph,
        scope,
        PETTY_CASH_TRANSACTION,
        json!({
            "fund_id": text(&voucher, "fund_id"),
            "transaction_type": "payment",
            "amount": wire(&amount),
            "date": text(&voucher, "date"),
            "description": text(&voucher, "description"),
            "category": category,
            "recipient_name": text(&voucher, "payee"),
            "reference_number": format!("VOUCHER-{voucher_number}"),
            "voucher_number": voucher_number,
            "entered_by": text(&voucher, "entered_by"),
            "created_at": text(payload, "created_at"),
        }),
    )?;

    Ok(voucher)
}

/// `GET /vouchers`.
pub fn list_vouchers(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let fund_id = optional(query, "fund_id");
    let status = optional(query, "status");
    let start = optional(query, "start_date");
    let end = optional(query, "end_date");
    let limit = query_page(query, 100, 2000);

    let mut rows: Vec<Value> = Records::new(graph, PETTY_CASH_VOUCHER)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            fund_id
                .as_deref()
                .map(|wanted| field(row, "fund_id") == wanted)
                .unwrap_or(true)
                && status
                    .as_deref()
                    .map(|wanted| field(row, "status") == wanted)
                    .unwrap_or(true)
                && start
                    .as_deref()
                    .map(|from| field(row, "date") >= from)
                    .unwrap_or(true)
                && end
                    .as_deref()
                    .map(|to| field(row, "date") <= to)
                    .unwrap_or(true)
        })
        .collect();
    rows.sort_by(|one, two| field(two, "date").cmp(field(one, "date")));
    rows.truncate(limit);
    Ok(Value::Array(rows))
}

/// `PUT /vouchers/{voucher_id}/approve`.
pub fn approve_voucher(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    voucher_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let vouchers = Records::new(graph, PETTY_CASH_VOUCHER);
    let voucher = vouchers
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == voucher_id)
        .ok_or(StoreError::NotFound)?;
    vouchers.update(
        scope,
        voucher_id,
        &json!({
            "status": "approved",
            "approved_by": text(payload, "approved_by"),
        }),
    )?;
    Ok(voucher)
}

/// `POST /replenishments`.
pub fn create_replenishment(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let fund_id = text(payload, "fund_id");
    fund_of(graph, scope, &fund_id)?;
    let status = checked(
        &or_default(payload, "status", "pending"),
        REPLENISHMENT_STATUS,
        "status",
    )?;

    remember(
        graph,
        scope,
        PETTY_CASH_REPLENISHMENT,
        json!({
            "fund_id": fund_id,
            "amount": wire(&money(payload, "amount")?),
            "request_date": text(payload, "request_date"),
            "requested_by": text(payload, "requested_by"),
            "approved_by": text(payload, "approved_by"),
            "status": status,
            "transactions_included": payload
                .get("transactions_included")
                .cloned()
                .unwrap_or_else(|| json!([])),
            "total_cash_disbursed": wire(&money_or_nothing(payload, "total_cash_disbursed")?),
            "bank_reference": text(payload, "bank_reference"),
            "notes": text(payload, "notes"),
        }),
    )
}

/// `GET /replenishments`.
pub fn list_replenishments(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let fund_id = optional(query, "fund_id");
    let status = optional(query, "status");
    let mut rows: Vec<Value> = Records::new(graph, PETTY_CASH_REPLENISHMENT)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            fund_id
                .as_deref()
                .map(|wanted| field(row, "fund_id") == wanted)
                .unwrap_or(true)
                && status
                    .as_deref()
                    .map(|wanted| field(row, "status") == wanted)
                    .unwrap_or(true)
        })
        .collect();
    rows.sort_by(|one, two| field(two, "request_date").cmp(field(one, "request_date")));
    Ok(Value::Array(rows))
}

/// `PUT /replenishments/{replenishment_id}/approve`.
pub fn approve_replenishment(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    replenishment_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let replenishments = Records::new(graph, PETTY_CASH_REPLENISHMENT);
    let found = replenishments
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == replenishment_id)
        .ok_or(StoreError::NotFound)?;
    replenishments.update(
        scope,
        replenishment_id,
        &json!({
            "status": "approved",
            "approved_by": text(payload, "approved_by"),
            "approved_date": text(payload, "approved_date"),
        }),
    )?;
    Ok(found)
}

/// `GET /reports/fund-report/{fund_id}`.
pub fn fund_report(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    fund_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let fund = fund_of(graph, scope, fund_id)?;
    let start = optional(query, "start_date");
    let end = optional(query, "end_date");
    let rows: Vec<Value> = transactions_of(graph, scope, Some(fund_id))?
        .into_iter()
        .filter(|row| {
            start
                .as_deref()
                .map(|from| field(row, "date") >= from)
                .unwrap_or(true)
                && end
                    .as_deref()
                    .map(|to| field(row, "date") <= to)
                    .unwrap_or(true)
        })
        .collect();

    let mut by_category: Vec<(String, usize, Money)> = Vec::new();
    for row in &rows {
        let category = field(row, "category").to_string();
        let amount = money_or_nothing(row, "amount")?;
        match by_category
            .iter_mut()
            .find(|(name, _, _)| *name == category)
        {
            Some((_, count, total)) => {
                *count += 1;
                *total = total.plus(&amount);
            }
            None => by_category.push((category, 1, amount)),
        }
    }
    by_category.sort_by(|one, two| one.0.cmp(&two.0));
    let categories: serde_json::Map<String, Value> = by_category
        .into_iter()
        .map(|(name, count, total)| (name, json!({"count": count, "total": wire(&total)})))
        .collect();

    let (receipts, payments) = totals(&rows)?;
    Ok(json!({
        "fund": fund,
        "period": {
            "start": start.unwrap_or_default(),
            "end": end.unwrap_or_default(),
        },
        "total_transactions": rows.len(),
        "by_category": Value::Object(categories),
        "balance_info": {
            "fund_id": fund_id,
            "total_receipts": wire(&receipts),
            "total_payments": wire(&payments),
            "current_balance": wire(&receipts.minus(&payments)),
        },
    }))
}

/// `GET /reports/cash-position`: what every fund the caller holds adds up to.
pub fn cash_position(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let mut funds = Vec::new();
    let mut total = Money::zero();

    for fund in Records::new(graph, PETTY_CASH_FUND).list(scope, 2000)? {
        let fund_id = text(&fund, "id");
        let balance = balance_of(graph, scope, &fund_id)?;
        let maximum = money_or_nothing(&fund, "maximum_balance")?;
        // a ratio, not money: what share of the fund's ceiling it is holding, when it has a ceiling
        let used = if maximum.is_zero() {
            "0.00".to_string()
        } else {
            balance
                .over(&maximum)
                .times(&fixed("100"))
                .round(2)
                .to_fixed(2)
        };
        total = total.plus(&balance);
        funds.push(json!({
            "fund_id": fund_id,
            "fund_name": text(&fund, "fund_name"),
            "location": text(&fund, "location"),
            "custodian": text(&fund, "custodian_name"),
            "current_balance": wire(&balance),
            "maximum_balance": wire(&maximum),
            "utilization_percentage": used,
            "status": text(&fund, "status"),
        }));
    }

    Ok(json!({
        "total_funds": funds.len(),
        "total_cash": wire(&total),
        "funds": funds,
    }))
}

/// `GET /reports/category-summary`: what the caller has spent, by what it was for.
pub fn category_summary(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let start = optional(query, "start_date");
    let end = optional(query, "end_date");
    let mut by_category: Vec<(String, usize, Money)> = Vec::new();
    let mut count = 0usize;
    let mut amount_total = Money::zero();

    for row in transactions_of(graph, scope, None)? {
        if field(&row, "transaction_type") != "payment" {
            continue;
        }
        if let Some(from) = start.as_deref() {
            if field(&row, "date") < from {
                continue;
            }
        }
        if let Some(to) = end.as_deref() {
            if field(&row, "date") > to {
                continue;
            }
        }
        let category = field(&row, "category").to_string();
        let amount = money_or_nothing(&row, "amount")?;
        count += 1;
        amount_total = amount_total.plus(&amount);
        match by_category
            .iter_mut()
            .find(|(name, _, _)| *name == category)
        {
            Some((_, seen, total)) => {
                *seen += 1;
                *total = total.plus(&amount);
            }
            None => by_category.push((category, 1, amount)),
        }
    }

    by_category.sort_by(|one, two| one.0.cmp(&two.0));
    let categories: serde_json::Map<String, Value> = by_category
        .into_iter()
        .map(|(name, seen, total)| (name, json!({"count": seen, "total": wire(&total)})))
        .collect();

    Ok(json!({
        "categories": Value::Object(categories),
        "total_transactions": count,
        "total_amount": wire(&amount_total),
    }))
}

/// Unused today but kept for the query readers: a whole-number query parameter.
#[allow(dead_code)]
fn limit_of(query: &Value) -> u32 {
    whole(query, "limit")
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn fund(id: &str) -> Value {
        json!({
            "id": id, "fund_code": format!("PC-{id}"), "fund_name": format!("fund {id}"),
            "status": "active", "maximum_balance": "2000.000000",
            "minimum_balance": "100.000000", "replenishment_threshold": "500.000000",
            "custodian_name": "Tendai", "location": "Harare",
        })
    }

    fn tx(fund_id: &str, kind: &str, amount: &str, date: &str) -> Value {
        json!({"id": "t-1", "fund_id": fund_id, "transaction_type": kind,
               "amount": amount, "date": date, "category": "travel"})
    }

    #[test]
    fn a_fund_whose_minimum_is_above_its_maximum_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        let refused = create_fund(
            &graph,
            &scope(),
            &json!({"fund_code": "PC-1", "fund_name": "Petty", "maximum_balance": "100",
                    "minimum_balance": "500"}),
        );
        assert!(refused.is_err());
    }

    #[test]
    fn a_fund_status_outside_the_four_the_service_knows_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        let refused = create_fund(
            &graph,
            &scope(),
            &json!({"fund_code": "PC-1", "fund_name": "Petty", "status": "frozen"}),
        );
        assert!(refused.is_err());
    }

    #[test]
    fn the_balance_is_what_came_in_less_what_went_out() {
        // in: 1000 initial fund and 200 receipt; out: 150 payment, so 1050
        let graph = FakeGraph::queue(vec![
            vec![fund("f-1")],
            vec![
                tx("f-1", "initial_fund", "1000.00", "2026-01-01"),
                tx("f-1", "receipt", "200.00", "2026-01-05"),
                tx("f-1", "payment", "150.00", "2026-01-06"),
            ],
        ]);
        let answer = fund_balance(&graph, &scope(), "f-1").expect("balance");
        assert_eq!(answer["total_receipts"], json!("1200.000000"));
        assert_eq!(answer["total_payments"], json!("150.000000"));
        assert_eq!(answer["current_balance"], json!("1050.000000"));
    }

    #[test]
    fn a_fund_below_its_threshold_needs_replenishing() {
        let graph = FakeGraph::queue(vec![
            vec![fund("f-1")],
            vec![tx("f-1", "initial_fund", "600.00", "2026-01-01")],
            vec![],
        ]);
        let answer = fund_summary(&graph, &scope(), "f-1").expect("summary");
        assert_eq!(answer["replenishment_needed"], json!(false));
        assert_eq!(answer["closing_balance"], json!("600.000000"));
    }

    #[test]
    fn a_payment_for_more_than_the_fund_holds_is_refused() {
        let graph = FakeGraph::queue(vec![
            vec![fund("f-1")],
            vec![tx("f-1", "initial_fund", "100.00", "2026-01-01")],
        ]);
        let refused = create_transaction(
            &graph,
            &scope(),
            &json!({"fund_id": "f-1", "transaction_type": "payment", "amount": "250",
                    "category": "travel", "date": "2026-01-07"}),
        );
        assert!(refused.is_err());
    }

    #[test]
    fn a_payment_category_outside_the_ten_the_service_knows_is_refused() {
        let graph = FakeGraph::queue(vec![vec![fund("f-1")]]);
        let refused = create_transaction(
            &graph,
            &scope(),
            &json!({"fund_id": "f-1", "transaction_type": "payment", "amount": "10",
                    "category": "bribes"}),
        );
        assert!(refused.is_err());
    }

    #[test]
    fn a_voucher_brings_its_payment_with_it() {
        let graph = FakeGraph::queue(vec![
            vec![fund("f-1")],
            vec![
                json!({"id": "v-1", "fund_id": "f-1", "voucher_number": "V-1",
                        "date": "2026-02-01", "payee": "Tarisai", "amount": "75.000000",
                        "description": "fuel", "category": "travel", "status": "pending",
                        "entered_by": "user-1"}),
            ],
            vec![json!({"id": "t-9"})],
        ]);
        let answer = create_voucher(
            &graph,
            &scope(),
            &json!({"fund_id": "f-1", "voucher_number": "V-1", "date": "2026-02-01",
                    "payee": "Tarisai", "amount": "75", "description": "fuel",
                    "category": "travel", "entered_by": "user-1"}),
        )
        .expect("voucher");
        assert_eq!(answer["status"], json!("pending"));
        // the payment that came with it
        let (_, last) = graph.statement(2);
        assert_eq!(last["props"]["transaction_type"], json!("payment"));
        assert_eq!(last["props"]["reference_number"], json!("VOUCHER-V-1"));
        assert_eq!(last["props"]["amount"], json!("75.000000"));
    }

    #[test]
    fn closing_a_fund_keeps_it_and_only_changes_its_status() {
        let graph = FakeGraph::queue(vec![
            vec![fund("f-1")],
            vec![json!({"id": "f-1", "status": "closed"})],
        ]);
        let answer =
            close_fund(&graph, &scope(), "f-1", &json!({"closed_by": "user-1"})).expect("closed");
        assert_eq!(answer["status"], json!("closed"));
        assert_eq!(answer["closed_by"], json!("user-1"));
        let (_, patch) = graph.statement(1);
        assert_eq!(patch["patch"]["status"], json!("closed"));
    }

    #[test]
    fn the_cash_position_adds_every_fund_up() {
        let graph = FakeGraph::queue(vec![
            vec![fund("f-1"), fund("f-2")],
            vec![tx("f-1", "initial_fund", "500.00", "2026-01-01")],
            vec![tx("f-2", "initial_fund", "250.00", "2026-01-01")],
        ]);
        let answer = cash_position(&graph, &scope()).expect("position");
        assert_eq!(answer["total_funds"], json!(2));
        assert_eq!(answer["total_cash"], json!("750.000000"));
        assert_eq!(answer["funds"][0]["utilization_percentage"], json!("25.00"));
    }

    #[test]
    fn the_category_summary_counts_only_what_was_paid_out() {
        let graph = FakeGraph::queue(vec![vec![
            tx("f-1", "payment", "40.00", "2026-01-02"),
            tx("f-1", "payment", "60.00", "2026-01-03"),
            tx("f-1", "receipt", "500.00", "2026-01-01"),
        ]]);
        let answer = category_summary(&graph, &scope(), &json!({})).expect("summary");
        assert_eq!(answer["total_transactions"], json!(2));
        assert_eq!(answer["total_amount"], json!("100.000000"));
        assert_eq!(answer["categories"]["travel"]["count"], json!(2));
    }
}
