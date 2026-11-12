#![forbid(unsafe_code)]
//! cashbook-service, on the store.
//!
//! Cash accounts, the receipts and payments written against them, the bank reconciliations that check
//! them, cash-flow entries, and the derived summaries. Every figure is exact: an amount is read as a
//! decimal, base amounts are computed in decimals, and balances are summed in decimals, so a long
//! balance keeps its digits where the original floats would have drifted.
//!
//! The rules, as shipped:
//!
//! * an account is opened at its opening balance, and the balance the client sends with it is ignored,
//! * an account code is unique within a Book,
//! * writing an entry against an account moves that account's balance immediately: a debit adds, a
//!   credit subtracts, by the base amount, which is the amount itself in USD and the amount times the
//!   exchange rate otherwise,
//! * an entry is stored with the status the client sent, but always as unreconciled,
//! * the summary counts only posted entries in its debit and credit totals and its closing balance,
//!   while its transaction count covers every entry, so the closing balance will not agree with the
//!   account balance while entries are pending,
//! * posting and voiding do not check what state the entry is already in: a voided entry can be posted,
//!   and voiding twice reverses the balance twice,
//! * a cash-position history starts its running balance at zero rather than at any account's opening
//!   balance, and the cash-flow summary's variance is expected minus actual while its net change is
//!   actual minus expected.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount, amount_or_zero, stored_amount, wire};
use vimbai_store::{
    created_id, BookScope, Graph, Records, StoreError, CASH_BANK_ACCOUNT, CASH_ENTRY,
    CASH_FLOW_ENTRY, CASH_RECONCILIATION,
};

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn optional(payload: &Value, key: &str) -> Option<String> {
    let given = text(payload, key);
    if given.is_empty() {
        None
    } else {
        Some(given)
    }
}

fn or_default(payload: &Value, key: &str, fallback: &str) -> String {
    optional(payload, key).unwrap_or_else(|| fallback.to_string())
}

fn flag(payload: &Value, key: &str) -> bool {
    payload.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// An optional amount: absent is absent, not zero.
fn optional_amount(payload: &Value, key: &str) -> Result<Option<Money>, StoreError> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(None),
        _ => amount(payload, key).map(Some),
    }
}

fn stored_optional(record: &Value, key: &str) -> Result<Option<Money>, StoreError> {
    match record.get(key) {
        None | Some(Value::Null) => Ok(None),
        _ => stored_amount(record, key).map(Some),
    }
}

fn field<'a>(record: &'a Value, key: &str) -> &'a str {
    record.get(key).and_then(Value::as_str).unwrap_or("")
}

fn is_true(record: &Value, key: &str) -> bool {
    record.get(key).and_then(Value::as_bool).unwrap_or(false)
}

// --- accounts ---

/// `/accounts`: a code may only be used once per Book, and the opening balance is the balance.
pub fn create_account<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, CASH_BANK_ACCOUNT);
    let code = text(payload, "account_code");
    let taken = records
        .list(scope, 1000)?
        .into_iter()
        .any(|row| field(&row, "account_code") == code);
    if taken {
        return Err(StoreError::Invalid(format!(
            "Account code {code} already exists"
        )));
    }
    let opening = amount_or_zero(payload, "opening_balance")?;
    records.create(
        scope,
        &json!({
            "account_code": code,
            "account_name": text(payload, "account_name"),
            "account_type": or_default(payload, "account_type", "bank"),
            "bank_name": text(payload, "bank_name"),
            "account_number": text(payload, "account_number"),
            "currency": or_default(payload, "currency", "USD"),
            "opening_balance": wire(&opening),
            "current_balance": wire(&opening),
            "is_active": payload.get("is_active").and_then(Value::as_bool).unwrap_or(true),
            "reconciliation_enabled": payload
                .get("reconciliation_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        }),
    )
}

/// `/accounts` listing with the service's filters.
pub fn list_accounts<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let kind = optional(query, "account_type");
    let currency = optional(query, "currency");
    let active = vimbai_store::query::query_flag(query, "is_active");
    let rows = Records::new(graph, CASH_BANK_ACCOUNT).list(scope, 1000)?;
    let kept: Vec<Value> = rows
        .into_iter()
        .filter(|row| {
            kind.as_deref()
                .map(|want| field(row, "account_type") == want)
                .unwrap_or(true)
                && currency
                    .as_deref()
                    .map(|want| field(row, "currency") == want)
                    .unwrap_or(true)
                && active
                    .map(|want| is_true(row, "is_active") == want)
                    .unwrap_or(true)
        })
        .collect();
    Ok(Value::Array(kept))
}

/// `/accounts/{id}`.
pub fn get_account<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, CASH_BANK_ACCOUNT).get(scope, account_id)
}

/// `/accounts/{id}` update. Unlike opening a new one, the balance sent here is taken as given.
pub fn update_account<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, CASH_BANK_ACCOUNT);
    let existing = records.get(scope, account_id)?;
    let opening = payload
        .get("opening_balance")
        .map(|_| amount_or_zero(payload, "opening_balance"))
        .transpose()?
        .unwrap_or(stored_amount(&existing, "opening_balance")?);
    let current = payload
        .get("current_balance")
        .map(|_| amount_or_zero(payload, "current_balance"))
        .transpose()?
        .unwrap_or(stored_amount(&existing, "current_balance")?);
    let mut patch = json!({
        "account_name": if payload.get("account_name").is_some() {
            text(payload, "account_name")
        } else {
            field(&existing, "account_name").to_string()
        },
        "account_type": or_default(payload, "account_type", field(&existing, "account_type")),
        "bank_name": text(payload, "bank_name"),
        "account_number": text(payload, "account_number"),
        "currency": or_default(payload, "currency", field(&existing, "currency")),
        "opening_balance": wire(&opening),
        "current_balance": wire(&current),
        "is_active": payload
            .get("is_active")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| is_true(&existing, "is_active")),
        "reconciliation_enabled": payload
            .get("reconciliation_enabled")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| is_true(&existing, "reconciliation_enabled")),
    });
    if patch.get("account_type") == Some(&json!("")) {
        patch["account_type"] = json!("bank");
    }
    records.update(scope, account_id, &patch)
}

fn account_by_code<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    code: &str,
) -> Result<Option<Value>, StoreError> {
    Ok(Records::new(graph, CASH_BANK_ACCOUNT)
        .list(scope, 1000)?
        .into_iter()
        .find(|row| field(row, "account_code") == code))
}

fn move_balance<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    code: &str,
    delta: &Money,
) -> Result<(), StoreError> {
    if code.is_empty() {
        return Ok(());
    }
    let Some(account) = account_by_code(graph, scope, code)? else {
        return Ok(());
    };
    let balance = stored_amount(&account, "current_balance")?.plus(delta);
    let id = field(&account, "id").to_string();
    Records::new(graph, CASH_BANK_ACCOUNT).update(
        scope,
        &id,
        &json!({ "current_balance": wire(&balance) }),
    )?;
    Ok(())
}

// --- entries ---

/// `/entries`. The base amount is the amount, converted when the currency is not USD.
pub fn create_entry<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let amount_value = amount(payload, "amount")?;
    let currency = or_default(payload, "currency", "USD");
    let rate = if payload.get("exchange_rate").is_some() {
        amount(payload, "exchange_rate")?
    } else {
        Money::parse("1").map_err(|error| StoreError::Invalid(format!("{error}")))?
    };
    let base = if currency == "USD" {
        amount_value.clone()
    } else {
        amount_value.times(&rate)
    };

    let debit = flag(payload, "is_debit");
    let delta = if debit { base.clone() } else { base.negate() };
    // the account moves first, as it did before the entry was written
    move_balance(graph, scope, &text(payload, "bank_account"), &delta)?;

    Records::new(graph, CASH_ENTRY).create(
        scope,
        &json!({
            "book_type": or_default(payload, "book_type", "receipts"),
            "entry_date": text(payload, "entry_date"),
            "voucher_number": text(payload, "voucher_number"),
            "description": text(payload, "description"),
            "account_code": text(payload, "account_code"),
            "amount": wire(&amount_value),
            "is_debit": debit,
            "reference": text(payload, "reference"),
            "cheque_number": text(payload, "cheque_number"),
            "bank_account": text(payload, "bank_account"),
            "currency": currency,
            "exchange_rate": wire(&rate),
            "base_amount": wire(&base),
            "narration": text(payload, "narration"),
            "posted_by": text(payload, "posted_by"),
            "status": or_default(payload, "status", "pending"),
            // always unreconciled, whatever the client sent
            "reconciled": false,
            "reconciliation_id": Value::Null,
        }),
    )
}

/// `/entries` listing, newest first.
pub fn list_entries<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let book_type = optional(query, "book_type");
    let account = optional(query, "bank_account");
    let status = optional(query, "status");
    let start = optional(query, "start_date");
    let end = optional(query, "end_date");
    let reconciled = vimbai_store::query::query_flag(query, "reconciled");
    let limit = vimbai_store::query::query_page(query, 100, 1000);

    let rows = Records::new(graph, CASH_ENTRY).list(scope, 1000)?;
    let mut kept: Vec<Value> = rows
        .into_iter()
        .filter(|row| {
            book_type
                .as_deref()
                .map(|want| field(row, "book_type") == want)
                .unwrap_or(true)
                && account
                    .as_deref()
                    .map(|want| field(row, "bank_account") == want)
                    .unwrap_or(true)
                && status
                    .as_deref()
                    .map(|want| field(row, "status") == want)
                    .unwrap_or(true)
                && start
                    .as_deref()
                    .map(|want| field(row, "entry_date") >= want)
                    .unwrap_or(true)
                && end
                    .as_deref()
                    .map(|want| field(row, "entry_date") <= want)
                    .unwrap_or(true)
                && reconciled
                    .map(|want| is_true(row, "reconciled") == want)
                    .unwrap_or(true)
        })
        .collect();
    kept.sort_by(|left, right| field(right, "entry_date").cmp(field(left, "entry_date")));
    kept.truncate(limit);
    Ok(Value::Array(kept))
}

/// `/entries/{id}`.
pub fn get_entry<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    entry_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, CASH_ENTRY).get(scope, entry_id)
}

/// `/entries/{id}/post`. No check on the state it is coming from.
pub fn post_entry<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    entry_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, CASH_ENTRY);
    records.get(scope, entry_id)?;
    let mut patch = json!({ "status": "posted" });
    if let Some(posted_by) = optional(payload, "posted_by") {
        patch["posted_by"] = json!(posted_by);
    }
    records.update(scope, entry_id, &patch)?;
    records.get(scope, entry_id)
}

/// `/entries/{id}/void`: reverses the balance and marks the entry voided.
pub fn void_entry<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    entry_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, CASH_ENTRY);
    let entry = records.get(scope, entry_id)?;
    let base = stored_amount(&entry, "base_amount")?;
    let undo = if is_true(&entry, "is_debit") {
        base.negate()
    } else {
        base
    };
    move_balance(graph, scope, field(&entry, "bank_account"), &undo)?;
    records.update(scope, entry_id, &json!({ "status": "voided" }))?;
    Ok(json!({
        "status": "voided",
        "entry_id": entry_id,
        "reason": text(payload, "reason"),
    }))
}

/// `/summary/{account_code}`.
pub fn summary<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account_code: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let Some(account) = account_by_code(graph, scope, account_code)? else {
        return Err(StoreError::NotFound);
    };
    let start = optional(query, "start_date");
    let end = optional(query, "end_date");
    let entries: Vec<Value> = Records::new(graph, CASH_ENTRY)
        .list(scope, 10000)?
        .into_iter()
        .filter(|row| field(row, "bank_account") == account_code)
        .filter(|row| {
            start
                .as_deref()
                .map(|want| field(row, "entry_date") >= want)
                .unwrap_or(true)
                && end
                    .as_deref()
                    .map(|want| field(row, "entry_date") <= want)
                    .unwrap_or(true)
        })
        .collect();

    let mut debits = Money::zero();
    let mut credits = Money::zero();
    let mut last: Option<&str> = None;
    for row in &entries {
        if field(row, "status") != "posted" {
            continue;
        }
        let base = stored_amount(row, "base_amount")?;
        if is_true(row, "is_debit") {
            debits = debits.plus(&base);
        } else {
            credits = credits.plus(&base);
        }
    }
    for row in &entries {
        let date = field(row, "entry_date");
        if last.map(|seen| date > seen).unwrap_or(true) {
            last = Some(date);
        }
    }

    let opening = stored_amount(&account, "opening_balance")?;
    let closing = opening.plus(&debits).minus(&credits);

    Ok(json!({
        "account_code": account_code,
        "account_name": field(&account, "account_name"),
        "opening_balance": wire(&opening),
        "total_debits": wire(&debits),
        "total_credits": wire(&credits),
        "closing_balance": wire(&closing),
        // every entry, not just the posted ones the totals count
        "transaction_count": entries.len(),
        "last_transaction_date": last,
    }))
}

// --- reconciliations ---

/// `/reconciliations`: the book balance is the account's opening balance plus its posted entries, and
/// the adjusted balance is the statement balance plus the adjustments supplied.
pub fn create_reconciliation<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let account_code = text(payload, "bank_account");
    let mut book_balance = Money::zero();
    for row in Records::new(graph, CASH_ENTRY).list(scope, 10000)? {
        if field(&row, "bank_account") != account_code || field(&row, "status") != "posted" {
            continue;
        }
        let base = stored_amount(&row, "base_amount")?;
        book_balance = if is_true(&row, "is_debit") {
            book_balance.plus(&base)
        } else {
            book_balance.minus(&base)
        };
    }
    if let Some(account) = account_by_code(graph, scope, &account_code)? {
        book_balance = stored_amount(&account, "opening_balance")?.plus(&book_balance);
    }

    let statement = amount(payload, "statement_balance")?;
    let adjustments = payload
        .get("adjustments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut adjustment_total = Money::zero();
    for item in &adjustments {
        adjustment_total = adjustment_total.plus(&amount_or_zero(item, "amount")?);
    }
    let adjusted = statement.plus(&adjustment_total);

    Records::new(graph, CASH_RECONCILIATION).create(
        scope,
        &json!({
            "bank_account": account_code,
            "statement_date": text(payload, "statement_date"),
            "statement_balance": wire(&statement),
            "book_balance": wire(&book_balance),
            "adjustments": adjustments,
            "adjusted_balance": wire(&adjusted),
            "differences": payload.get("differences").cloned().unwrap_or_else(|| json!([])),
            "status": "in_progress",
            "prepared_by": text(payload, "prepared_by"),
            "reviewed_by": Value::Null,
            "completed_at": Value::Null,
        }),
    )
}

/// `/reconciliations` listing, newest statement first.
pub fn list_reconciliations<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let account = optional(query, "bank_account");
    let status = optional(query, "status");
    let mut kept: Vec<Value> = Records::new(graph, CASH_RECONCILIATION)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| {
            account
                .as_deref()
                .map(|want| field(row, "bank_account") == want)
                .unwrap_or(true)
                && status
                    .as_deref()
                    .map(|want| field(row, "status") == want)
                    .unwrap_or(true)
        })
        .collect();
    kept.sort_by(|left, right| field(right, "statement_date").cmp(field(left, "statement_date")));
    Ok(Value::Array(kept))
}

/// `/reconciliations/{id}`.
pub fn get_reconciliation<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    reconciliation_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, CASH_RECONCILIATION).get(scope, reconciliation_id)
}

/// `/reconciliations/{id}/complete`: completes it and reconciles the entries its adjustments name.
pub fn complete_reconciliation<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    reconciliation_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, CASH_RECONCILIATION);
    let existing = records.get(scope, reconciliation_id)?;
    let reviewed_by = text(payload, "reviewed_by");
    records.update(
        scope,
        reconciliation_id,
        &json!({
            "status": "completed",
            "reviewed_by": reviewed_by,
            "completed_at": text(payload, "completed_at"),
        }),
    )?;

    let entries = Records::new(graph, CASH_ENTRY);
    for adjustment in existing
        .get("adjustments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let Some(entry_id) = optional(&adjustment, "entry_id") else {
            continue;
        };
        if entries.get(scope, &entry_id).is_err() {
            continue;
        }
        entries.update(
            scope,
            &entry_id,
            &json!({"reconciled": true, "reconciliation_id": reconciliation_id}),
        )?;
    }
    records.get(scope, reconciliation_id)
}

/// `/reconciliations/{id}/outstanding`.
pub fn outstanding_items<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    reconciliation_id: &str,
) -> Result<Value, StoreError> {
    let reconciliation = Records::new(graph, CASH_RECONCILIATION).get(scope, reconciliation_id)?;
    let account_code = field(&reconciliation, "bank_account").to_string();
    let mut rows: Vec<Value> = Records::new(graph, CASH_ENTRY)
        .list(scope, 10000)?
        .into_iter()
        .filter(|row| {
            field(row, "bank_account") == account_code
                && !is_true(row, "reconciled")
                && field(row, "status") == "posted"
        })
        .collect();
    rows.sort_by(|left, right| field(right, "entry_date").cmp(field(left, "entry_date")));

    let mut debits = Money::zero();
    let mut credits = Money::zero();
    for row in &rows {
        let base = stored_amount(row, "base_amount")?;
        if is_true(row, "is_debit") {
            debits = debits.plus(&base);
        } else {
            credits = credits.plus(&base);
        }
    }
    Ok(json!({
        "reconciliation_id": reconciliation_id,
        "total_items": rows.len(),
        "total_debits": wire(&debits),
        "total_credits": wire(&credits),
        "difference": wire(&debits.minus(&credits)),
        "entries": rows,
    }))
}

// --- cash flow ---

/// `/cash-flow`.
pub fn create_cash_flow_entry<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let expected = optional_amount(payload, "expected_amount")?;
    let actual = optional_amount(payload, "actual_amount")?;
    let variance = optional_amount(payload, "variance")?;
    Records::new(graph, CASH_FLOW_ENTRY).create(
        scope,
        &json!({
            "entry_date": text(payload, "entry_date"),
            "category": text(payload, "category"),
            "subcategory": text(payload, "subcategory"),
            "description": text(payload, "description"),
            "expected_amount": expected.as_ref().map(wire).unwrap_or(Value::Null),
            "actual_amount": actual.as_ref().map(wire).unwrap_or(Value::Null),
            "variance": variance.as_ref().map(wire).unwrap_or(Value::Null),
            "cash_flow_type": text(payload, "cash_flow_type"),
            "source": text(payload, "source"),
            "reference_id": text(payload, "reference_id"),
        }),
    )
}

/// `/cash-flow` listing, newest first.
pub fn list_cash_flow<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let kind = optional(query, "cash_flow_type");
    let category = optional(query, "category");
    let start = optional(query, "start_date");
    let end = optional(query, "end_date");
    let limit = vimbai_store::query::query_page(query, 100, 1000);

    let mut kept: Vec<Value> = Records::new(graph, CASH_FLOW_ENTRY)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| {
            kind.as_deref()
                .map(|want| field(row, "cash_flow_type") == want)
                .unwrap_or(true)
                && category
                    .as_deref()
                    .map(|want| field(row, "category") == want)
                    .unwrap_or(true)
                && start
                    .as_deref()
                    .map(|want| field(row, "entry_date") >= want)
                    .unwrap_or(true)
                && end
                    .as_deref()
                    .map(|want| field(row, "entry_date") <= want)
                    .unwrap_or(true)
        })
        .collect();
    kept.sort_by(|left, right| field(right, "entry_date").cmp(field(left, "entry_date")));
    kept.truncate(limit);
    Ok(Value::Array(kept))
}

fn period_rows<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    start: &str,
    end: &str,
) -> Result<Vec<Value>, StoreError> {
    Ok(Records::new(graph, CASH_FLOW_ENTRY)
        .list(scope, 10000)?
        .into_iter()
        .filter(|row| {
            let date = field(row, "entry_date");
            date >= start && date <= end
        })
        .collect())
}

fn bucket(rows: &[Value]) -> Result<Value, StoreError> {
    let mut expected = Money::zero();
    let mut actual = Money::zero();
    for row in rows {
        if let Some(value) = stored_optional(row, "actual_amount")? {
            actual = actual.plus(&value);
        }
        if let Some(value) = stored_optional(row, "expected_amount")? {
            expected = expected.plus(&value);
        }
    }
    Ok(json!({
        "count": rows.len(),
        "expected_total": wire(&expected),
        "actual_total": wire(&actual),
        "variance": wire(&expected.minus(&actual)),
    }))
}

/// `/cash-flow/summary`.
pub fn cash_flow_summary<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let start = text(query, "start_date");
    let end = text(query, "end_date");
    let rows = period_rows(graph, scope, &start, &end)?;
    let by_kind = |want: &str| -> Vec<Value> {
        rows.iter()
            .filter(|row| field(row, "cash_flow_type") == want)
            .cloned()
            .collect()
    };
    let operating = bucket(&by_kind("operating"))?;
    let investing = bucket(&by_kind("investing"))?;
    let financing = bucket(&by_kind("financing"))?;

    let mut actual_all = Money::zero();
    let mut expected_all = Money::zero();
    for row in &rows {
        if let Some(value) = stored_optional(row, "actual_amount")? {
            actual_all = actual_all.plus(&value);
        }
        if let Some(value) = stored_optional(row, "expected_amount")? {
            expected_all = expected_all.plus(&value);
        }
    }

    Ok(json!({
        "period": {"start": start, "end": end},
        "operating": operating,
        "investing": investing,
        "financing": financing,
        // actual less expected, the opposite sign to each bucket's variance
        "net_change": wire(&actual_all.minus(&expected_all)),
    }))
}

// --- position ---

/// `/cash-position`.
pub fn cash_position<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let as_of = or_default(query, "as_of_date", "");
    let mut total_cash = Money::zero();
    let mut total_bank = Money::zero();
    let mut total_savings = Money::zero();
    let mut total_current = Money::zero();
    let mut by_currency: Vec<(String, Money)> = Vec::new();
    let mut by_account = Vec::new();

    for account in Records::new(graph, CASH_BANK_ACCOUNT).list(scope, 1000)? {
        let balance = stored_amount(&account, "current_balance")?;
        let currency = or_default(&account, "currency", "USD");
        by_account.push(json!({
            "account_code": field(&account, "account_code"),
            "account_name": field(&account, "account_name"),
            "account_type": field(&account, "account_type"),
            "balance": wire(&balance),
            "currency": currency,
        }));
        match field(&account, "account_type") {
            "cash" => total_cash = total_cash.plus(&balance),
            "bank" => total_bank = total_bank.plus(&balance),
            "savings" => total_savings = total_savings.plus(&balance),
            "current" => total_current = total_current.plus(&balance),
            _ => {}
        }
        match by_currency.iter_mut().find(|(name, _)| name == &currency) {
            Some((_, sum)) => *sum = sum.plus(&balance),
            None => by_currency.push((currency, balance)),
        }
    }

    Ok(json!({
        "as_of_date": as_of,
        "total_cash": wire(&total_cash),
        "total_bank": wire(&total_bank),
        "total_savings": wire(&total_savings),
        "total_current": wire(&total_current),
        "by_currency": by_currency
            .into_iter()
            .map(|(currency, sum)| (currency, wire(&sum)))
            .collect::<serde_json::Map<String, Value>>(),
        "by_account": by_account,
    }))
}

/// `/cash-position/history`: a running balance built from the entries in the window, starting at zero.
pub fn cash_position_history<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let start = text(query, "start_date");
    let end = text(query, "end_date");
    let mut days: Vec<(String, Money, Money)> = Vec::new();
    for row in Records::new(graph, CASH_ENTRY).list(scope, 10000)? {
        let date = field(&row, "entry_date");
        if date < start.as_str() || date > end.as_str() {
            continue;
        }
        // the day, not the timestamp
        let day = date.split('T').next().unwrap_or(date).to_string();
        let base = stored_amount(&row, "base_amount")?;
        let index = match days.iter().position(|(seen, _, _)| seen == &day) {
            Some(index) => index,
            None => {
                days.push((day.clone(), Money::zero(), Money::zero()));
                days.len() - 1
            }
        };
        if is_true(&row, "is_debit") {
            days[index].1 = days[index].1.plus(&base);
        } else {
            days[index].2 = days[index].2.plus(&base);
        }
    }
    days.sort_by(|left, right| left.0.cmp(&right.0));

    let mut running = Money::zero();
    let mut history = Vec::new();
    for (day, debits, credits) in days {
        let net = debits.minus(&credits);
        running = running.plus(&net);
        history.push(json!({
            "date": day,
            "total_debits": wire(&debits),
            "total_credits": wire(&credits),
            "net_change": wire(&net),
            "running_balance": wire(&running),
        }));
    }

    Ok(json!({"start_date": start, "end_date": end, "history": history}))
}

/// The id of the record a create answered, for endpoints that return only that.
pub fn created_or_null(created: &Value) -> Value {
    created_id(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn account(code: &str, kind: &str, currency: &str, opening: &str, balance: &str) -> Value {
        json!({"id": format!("acc-{code}"), "account_code": code, "account_name": "Current",
               "account_type": kind, "currency": currency, "opening_balance": opening,
               "current_balance": balance, "is_active": true})
    }

    fn entry(id: &str, amount: &str, debit: bool, status: &str, date: &str) -> Value {
        json!({"id": id, "bank_account": "1000", "base_amount": amount, "is_debit": debit,
               "status": status, "entry_date": date, "reconciled": false})
    }

    #[test]
    fn a_new_account_opens_at_its_opening_balance_whatever_else_was_sent() {
        let graph = FakeGraph::queue(vec![vec![], vec![json!({"id": "acc-1"})]]);
        create_account(
            &graph,
            &scope(),
            &json!({"account_code": "1000", "account_name": "Current", "account_type": "bank",
                    "opening_balance": "123456789012345678901.55",
                    "current_balance": "1"}),
        )
        .expect("opened");
        let (_, params) = graph.statement(1);
        assert_eq!(
            params["props"]["opening_balance"],
            json!("123456789012345678901.550000")
        );
        assert_eq!(
            params["props"]["current_balance"],
            json!("123456789012345678901.550000")
        );
    }

    #[test]
    fn an_account_code_may_only_be_used_once() {
        let graph = FakeGraph::queue(vec![vec![account("1000", "bank", "USD", "0", "0")]]);
        assert!(matches!(
            create_account(
                &graph,
                &scope(),
                &json!({"account_code": "1000", "account_name": "Second"})
            ),
            Err(StoreError::Invalid(_))
        ));
    }

    #[test]
    fn a_debit_raises_the_account_and_a_credit_lowers_it() {
        let graph = FakeGraph::queue(vec![
            vec![account("1000", "bank", "USD", "500", "500")],
            vec![json!({"id": "acc-1000"})],
            vec![json!({"id": "e-1"})],
        ]);
        create_entry(
            &graph,
            &scope(),
            &json!({"book_type": "receipts", "entry_date": "2026-03-01T10:00:00Z",
                    "voucher_number": "R1", "description": "Sale", "account_code": "4000",
                    "amount": "250.25", "is_debit": true, "bank_account": "1000",
                    "posted_by": "clerk"}),
        )
        .expect("written");
        let (_, patch) = graph.statement(1);
        assert_eq!(patch["patch"]["current_balance"], json!("750.250000"));
    }

    #[test]
    fn a_foreign_entry_is_converted_before_it_touches_the_balance() {
        let graph = FakeGraph::queue(vec![
            vec![account("1000", "bank", "USD", "0", "0")],
            vec![json!({"id": "acc-1000"})],
            vec![json!({"id": "e-1"})],
        ]);
        create_entry(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-03-01T10:00:00Z", "amount": "100", "currency": "GBP",
                    "exchange_rate": "1.35", "is_debit": false, "bank_account": "1000"}),
        )
        .expect("written");
        let (_, patch) = graph.statement(1);
        assert_eq!(patch["patch"]["current_balance"], json!("-135.000000"));
        let (_, created) = graph.statement(2);
        assert_eq!(created["props"]["base_amount"], json!("135.000000"));
        assert_eq!(created["props"]["reconciled"], json!(false));
    }

    #[test]
    fn the_summary_counts_only_posted_entries_but_counts_every_entry() {
        let graph = FakeGraph::queue(vec![
            vec![account("1000", "bank", "USD", "100", "100")],
            vec![
                entry("e-1", "500", true, "posted", "2026-03-01T09:00:00Z"),
                entry("e-2", "200", false, "posted", "2026-03-02T09:00:00Z"),
                entry("e-3", "900", true, "pending", "2026-03-03T09:00:00Z"),
            ],
        ]);
        let answer = summary(&graph, &scope(), "1000", &json!({})).expect("summarised");
        assert_eq!(answer["total_debits"], json!("500.000000"));
        assert_eq!(answer["total_credits"], json!("200.000000"));
        // 100 + 500 - 200; the pending 900 is not in it
        assert_eq!(answer["closing_balance"], json!("400.000000"));
        // but all three are counted
        assert_eq!(answer["transaction_count"], json!(3));
        assert_eq!(
            answer["last_transaction_date"],
            json!("2026-03-03T09:00:00Z")
        );
    }

    #[test]
    fn a_void_reverses_the_balance_and_can_be_reversed_twice() {
        let mut voided = entry("e-1", "500", true, "posted", "2026-03-01T09:00:00Z");
        voided["bank_account"] = json!("1000");
        let graph = FakeGraph::queue(vec![
            vec![voided.clone()],
            vec![account("1000", "bank", "USD", "1000", "1000")],
            vec![json!({"id": "acc-1000"})],
            vec![json!({"id": "e-1"})],
            vec![voided],
            vec![account("1000", "bank", "USD", "1000", "500")],
            vec![json!({"id": "acc-1000"})],
            vec![json!({"id": "e-1"})],
        ]);
        let first = void_entry(&graph, &scope(), "e-1", &json!({"reason": "wrong account"}))
            .expect("voided");
        assert_eq!(first["status"], json!("voided"));
        let (_, patch) = graph.statement(2);
        assert_eq!(patch["patch"]["current_balance"], json!("500.000000"));
        void_entry(&graph, &scope(), "e-1", &json!({"reason": "again"})).expect("voided twice");
        let (_, second) = graph.statement(6);
        assert_eq!(second["patch"]["current_balance"], json!("0.000000"));
    }

    #[test]
    fn a_reconciliation_takes_the_statement_balance_plus_its_adjustments() {
        let graph = FakeGraph::queue(vec![
            vec![
                entry("e-1", "500", true, "posted", "2026-03-01T09:00:00Z"),
                entry("e-2", "200", false, "posted", "2026-03-02T09:00:00Z"),
                entry("e-3", "900", true, "pending", "2026-03-03T09:00:00Z"),
            ],
            vec![account("1000", "bank", "USD", "100", "100")],
            vec![json!({"id": "r-1"})],
        ]);
        create_reconciliation(
            &graph,
            &scope(),
            &json!({"bank_account": "1000", "statement_date": "2026-03-31T00:00:00Z",
                    "statement_balance": "1000", "prepared_by": "clerk",
                    "adjustments": [{"entry_id": "e-1", "amount": "25.50"},
                                    {"amount": "4.50"}]}),
        )
        .expect("reconciled");
        let (_, created) = graph.statement(2);
        // 100 opening + 500 debit - 200 credit, the pending entry left out
        assert_eq!(created["props"]["book_balance"], json!("400.000000"));
        assert_eq!(created["props"]["adjusted_balance"], json!("1030.000000"));
        assert_eq!(created["props"]["status"], json!("in_progress"));
    }

    #[test]
    fn completing_a_reconciliation_reconciles_the_entries_it_names() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "r-1", "bank_account": "1000", "status": "in_progress",
                        "adjustments": [{"entry_id": "e-1", "amount": "25"}]}),
            ],
            vec![json!({"id": "r-1"})],
            vec![entry("e-1", "500", true, "posted", "2026-03-01T09:00:00Z")],
            vec![json!({"id": "e-1"})],
            vec![json!({"id": "r-1", "status": "completed"})],
        ]);
        let answer = complete_reconciliation(
            &graph,
            &scope(),
            "r-1",
            &json!({"reviewed_by": "auditor", "completed_at": "2026-04-01T00:00:00Z"}),
        )
        .expect("completed");
        assert_eq!(answer["status"], json!("completed"));
        let (_, patch) = graph.statement(3);
        assert_eq!(patch["patch"]["reconciled"], json!(true));
        assert_eq!(patch["patch"]["reconciliation_id"], json!("r-1"));
    }

    #[test]
    fn the_cash_flow_summary_keeps_variance_and_net_change_the_way_they_were() {
        let graph = FakeGraph::queue(vec![vec![
            json!({"code": "a", "cash_flow_type": "operating", "expected_amount": "100",
                   "actual_amount": "80", "entry_date": "2026-03-01"}),
            json!({"code": "b", "cash_flow_type": "investing", "expected_amount": "50",
                   "actual_amount": "70", "entry_date": "2026-03-02"}),
        ]]);
        let answer = cash_flow_summary(
            &graph,
            &scope(),
            &json!({"start_date": "2026-03-01", "end_date": "2026-03-31"}),
        )
        .expect("summarised");
        assert_eq!(answer["operating"]["variance"], json!("20.000000"));
        assert_eq!(answer["investing"]["variance"], json!("-20.000000"));
        // actual 150 less expected 150
        assert_eq!(answer["net_change"], json!("0.000000"));
    }

    #[test]
    fn the_cash_position_groups_by_type_and_currency() {
        let graph = FakeGraph::returning(vec![
            account("1000", "bank", "USD", "0", "1000.55"),
            account("1010", "cash", "USD", "0", "20"),
            account("2000", "savings", "GBP", "0", "300"),
            account("2010", "current", "GBP", "0", "40"),
        ]);
        let answer = cash_position(&graph, &scope(), &json!({})).expect("positioned");
        assert_eq!(answer["total_bank"], json!("1000.550000"));
        assert_eq!(answer["total_cash"], json!("20.000000"));
        assert_eq!(answer["by_currency"]["USD"], json!("1020.550000"));
        assert_eq!(answer["by_currency"]["GBP"], json!("340.000000"));
        assert_eq!(answer["by_account"].as_array().expect("rows").len(), 4);
    }

    #[test]
    fn the_history_runs_from_zero_one_day_at_a_time() {
        let graph = FakeGraph::returning(vec![
            entry("e-1", "500", true, "posted", "2026-03-01T09:00:00Z"),
            entry("e-2", "200", false, "posted", "2026-03-01T17:00:00Z"),
            entry("e-3", "100", true, "posted", "2026-03-03T09:00:00Z"),
        ]);
        let answer = cash_position_history(
            &graph,
            &scope(),
            &json!({"start_date": "2026-03-01", "end_date": "2026-03-31"}),
        )
        .expect("history");
        let history = answer["history"].as_array().expect("days");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0]["date"], json!("2026-03-01"));
        assert_eq!(history[0]["net_change"], json!("300.000000"));
        assert_eq!(history[0]["running_balance"], json!("300.000000"));
        assert_eq!(history[1]["running_balance"], json!("400.000000"));
    }
}

#[cfg(test)]
mod query_tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    /// A page size arrives as text over HTTP, and a filter does too; both used to fall back to their
    /// defaults, so a caller asking for five entries was answered with a hundred.
    #[test]
    fn a_page_size_and_a_filter_read_from_a_query_string_are_honoured() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "e-1", "reconciled": true, "amount": "10.000000"}),
            json!({"id": "e-2", "reconciled": false, "amount": "20.000000"}),
        ]);
        let answered = list_entries(
            &graph,
            &BookScope::new("user-1", "book-1"),
            &json!({"reconciled": "true", "limit": "1"}),
        )
        .expect("entries");
        let rows = answered.as_array().expect("rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], json!("e-1"));
    }
}
