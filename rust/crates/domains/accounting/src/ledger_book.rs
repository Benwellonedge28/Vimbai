#![forbid(unsafe_code)]
//! accounting-service: the chart of accounts and the journal written against it.
//!
//! This is the book of record the other services describe. It owns two kinds of record, both
//! Book-scoped: a chart account, and a journal entry. An entry's lines live on the entry rather than
//! as separate nodes, so an entry is written and read as one immutable unit; that is the shape the
//! ledger's correction rule wants, and it is why every report here reads entries rather than lines.
//!
//! Amounts are exact end to end. Every debit and credit is parsed into [`Money`], summed exactly, and
//! stored as a decimal string, so a trial balance over a hundred billion units still adds up.
//!
//! The double-entry rules are the ledger kernel's, checked exactly rather than with the kernel's
//! floating-point tolerance: at least two lines, every line moving exactly one side by a positive
//! amount, and total debits equal to total credits to the last digit.
//!
//! The hash chain is the kernel's too. Each entry is stamped over its business fields and the stamp of
//! the entry before it, so a later change to an earlier entry breaks every stamp after it. New stamps
//! carry amounts as exact decimal strings. Entries written before this port were stamped with the
//! payload's amounts rendered as floating-point numbers, so verification recomputes each entry under
//! the exact rule first and falls back to the older rendering; an entry that only matches the older
//! rendering is reported as legacy-stamped rather than as a broken chain, because it is intact.
//!
//! Behaviours kept as shipped, each pinned:
//!
//! * only entries whose status is `posted` count towards a balance, a ledger, a trial balance or a
//!   statement; a draft or voided entry is kept but moves nothing;
//! * an account's balance is debits less credits when it is debit-normal and credits less debits
//!   otherwise, so an account whose normal side is not the literal `debit` is treated as credit-normal;
//! * a trial balance moves a negative balance to the other column, and calls itself balanced only when
//!   the two columns are equal to the last digit;
//! * the income statement reads the period's activity, not the closing balance, and leaves an account
//!   out entirely when its net movement runs against its natural side;
//! * the balance sheet lists every asset, liability and equity account whether or not anything moved,
//!   and carries no profit for the period, so it can legitimately fail to balance;
//! * an account is refused for deletion while any journal line touches it;
//! * a reference number is unique per source module, but only for the six modules the service checks.
//!
//! One behaviour deliberately changed: an entry's stamped fields are immutable. Editing a posted
//! entry used to be allowed, which silently broke its own stamp and every stamp after it, and corrections
//! belong in a reversing entry. A posted entry can therefore not be edited or deleted here, and a draft
//! can be deleted but still not edited.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_ledger_core::hash_chain::{
    chain_state_from_digests, compute_entry_hash, EntryDigest, GENESIS_HASH,
};
use vimbai_store::amounts::{amount, wire};
use vimbai_store::{
    created_record, BookScope, Graph, Records, StoreError, JOURNAL_ENTRY, LEDGER_ACCOUNT,
};

/// The source modules whose reference numbers the service keeps unique.
const REFERENCE_CHECKED: [&str; 6] = [
    "Banking",
    "Invoicing",
    "Multimodal",
    "SupplyChain",
    "Manual",
    "MobileApp",
];

fn refused(message: &str) -> StoreError {
    StoreError::Invalid(message.to_string())
}

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn optional(payload: &Value, key: &str) -> Option<String> {
    let value = text(payload, key);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn or_default(payload: &Value, key: &str, fallback: &str) -> String {
    optional(payload, key).unwrap_or_else(|| fallback.to_string())
}

fn field<'a>(record: &'a Value, key: &str) -> &'a str {
    record.get(key).and_then(Value::as_str).unwrap_or("")
}

fn zero() -> Money {
    Money::zero()
}

/// An amount read from a stored or supplied figure, exact. Anything unreadable is refused rather than
/// treated as zero, which is the difference between a missing debit and a free one.
fn money(value: &Value) -> Result<Money, StoreError> {
    amount(&json!({ "amount": value }), "amount")
}

fn money_at(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        Some(Value::Null) | None => Ok(zero()),
        Some(value) => money(value),
    }
}

fn is_zero(value: &Money) -> bool {
    *value == zero()
}

fn positive(value: &Money) -> bool {
    *value > zero() && !is_zero(value)
}

// --- the chart of accounts ---

fn account_body(payload: &Value, created_at: &str, updated_at: &str) -> Value {
    json!({
        "name": text(payload, "name"),
        "account_number": text(payload, "account_number"),
        "account_type": or_default(payload, "account_type", "asset"),
        "normal_balance": or_default(payload, "normal_balance", "debit"),
        "description": text(payload, "description"),
        "parent_account_number": payload
            .get("parent_account_number")
            .cloned()
            .unwrap_or(Value::Null),
        "created_at": created_at,
        "updated_at": updated_at,
    })
}

fn accounts<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    let mut rows = Records::new(graph, LEDGER_ACCOUNT).list(scope, 2000)?;
    rows.sort_by(|one, two| field(one, "account_number").cmp(field(two, "account_number")));
    Ok(rows)
}

fn account_by_number<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account_number: &str,
) -> Result<Option<Value>, StoreError> {
    Ok(accounts(graph, scope)?
        .into_iter()
        .find(|row| field(row, "account_number") == account_number))
}

fn require_account<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account_number: &str,
) -> Result<Value, StoreError> {
    account_by_number(graph, scope, account_number)?.ok_or(StoreError::NotFound)
}

/// `POST /accounts/`: a chart account, refused when the number is taken.
pub fn create_account<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let number = text(payload, "account_number");
    if account_by_number(graph, scope, &number)?.is_some() {
        return Err(refused(&format!(
            "Account number {number} already exists for this user."
        )));
    }
    let body = account_body(
        payload,
        &text(payload, "created_at"),
        &text(payload, "updated_at"),
    );
    let saved = Records::new(graph, LEDGER_ACCOUNT).create(scope, &body)?;
    Ok(merge(body, &saved))
}

fn merge(body: Value, saved: &Value) -> Value {
    let mut answer = created_record(saved);
    if let (Some(answer), Some(fields)) = (answer.as_object_mut(), body.as_object()) {
        for (key, value) in fields {
            answer.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    answer
}

/// `GET /accounts/{number}`.
pub fn get_account<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account_number: &str,
) -> Result<Value, StoreError> {
    require_account(graph, scope, account_number)
}

/// `GET /accounts/`, in account-number order.
pub fn list_accounts<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    Ok(Value::Array(accounts(graph, scope)?))
}

/// `PUT /accounts/{number}`: only the fields the caller sent change.
pub fn update_account<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account_number: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let existing = require_account(graph, scope, account_number)?;
    let mut body = existing.clone();
    for key in [
        "name",
        "account_type",
        "normal_balance",
        "description",
        "parent_account_number",
    ] {
        if let Some(value) = payload.get(key) {
            body[key] = value.clone();
        }
    }
    body["updated_at"] = json!(text(payload, "updated_at"));
    let body = account_body(
        &body,
        field(&existing, "created_at"),
        field(&existing, "updated_at"),
    );
    let records = Records::new(graph, LEDGER_ACCOUNT);
    records.delete(scope, field(&existing, "id"))?;
    let saved = records.create(scope, &body)?;
    Ok(merge(body, &saved))
}

fn entry_lines(entry: &Value) -> Vec<Value> {
    entry
        .get("lines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn touches_account(entry: &Value, account_number: &str) -> bool {
    entry_lines(entry)
        .iter()
        .any(|line| field(line, "account_number") == account_number)
}

/// `DELETE /accounts/{number}`: refused while a journal line touches it.
pub fn delete_account<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account_number: &str,
) -> Result<Value, StoreError> {
    let existing = require_account(graph, scope, account_number)?;
    for entry in Records::new(graph, JOURNAL_ENTRY).list(scope, 2000)? {
        if touches_account(&entry, account_number) {
            return Err(refused(
                "Account is linked to existing journal entries and cannot be deleted.",
            ));
        }
    }
    Records::new(graph, LEDGER_ACCOUNT).delete(scope, field(&existing, "id"))?;
    Ok(json!({ "ok": true }))
}

// --- the journal ---

fn entries<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, JOURNAL_ENTRY).list(scope, 4000)
}

fn posted(entry: &Value) -> bool {
    field(entry, "status") == "posted"
}

fn dated_on_or_after(entry: &Value, bound: &str) -> bool {
    field(entry, "entry_date") >= bound
}

fn dated_on_or_before(entry: &Value, bound: &str) -> bool {
    field(entry, "entry_date") <= bound
}

/// The movements an account saw, as (debit, credit) pairs, from posted entries only.
fn movements(
    entries: &[Value],
    account_number: &str,
    from: Option<&str>,
    until: Option<&str>,
) -> Result<Vec<(Money, Money)>, StoreError> {
    let mut rows = Vec::new();
    for entry in entries.iter().filter(|entry| posted(entry)) {
        if let Some(bound) = from {
            if !dated_on_or_after(entry, bound) {
                continue;
            }
        }
        if let Some(bound) = until {
            if !dated_on_or_before(entry, bound) {
                continue;
            }
        }
        for line in entry_lines(entry) {
            if field(&line, "account_number") != account_number {
                continue;
            }
            rows.push((money_at(&line, "debit")?, money_at(&line, "credit")?));
        }
    }
    Ok(rows)
}

fn totals(rows: &[(Money, Money)]) -> (Money, Money) {
    let mut debit = zero();
    let mut credit = zero();
    for (dr, cr) in rows {
        debit = debit.plus(dr);
        credit = credit.plus(cr);
    }
    (debit, credit)
}

fn balance_of(account: &Value, debit: &Money, credit: &Money) -> Money {
    if field(account, "normal_balance") == "debit" {
        debit.minus(credit)
    } else {
        credit.minus(debit)
    }
}

pub fn account_balance<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account: &Value,
    as_of: Option<&str>,
) -> Result<Money, StoreError> {
    let rows = movements(
        &entries(graph, scope)?,
        field(account, "account_number"),
        None,
        as_of,
    )?;
    let (debit, credit) = totals(&rows);
    Ok(balance_of(account, &debit, &credit))
}

/// `GET /accounts/{number}/period-activity`: what the account did between two dates.
pub fn account_period_activity<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account_number: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let account = require_account(graph, scope, account_number)?;
    let from = optional(query, "start_date").ok_or_else(|| refused("start_date is required"))?;
    let until = optional(query, "end_date").ok_or_else(|| refused("end_date is required"))?;
    let rows = movements(
        &entries(graph, scope)?,
        field(&account, "account_number"),
        Some(&from),
        Some(&until),
    )?;
    let (debit, credit) = totals(&rows);
    Ok(json!({
        "total_debits": wire(&debit),
        "total_credits": wire(&credit),
    }))
}

fn validated_lines(payload: &Value) -> Result<Vec<Value>, StoreError> {
    let rows = payload
        .get("lines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if rows.len() < 2 {
        return Err(refused("an entry needs at least two lines"));
    }

    let mut debit_total = zero();
    let mut credit_total = zero();
    let mut lines = Vec::new();
    for (index, line) in rows.iter().enumerate() {
        let debit = money_at(line, "debit")?;
        let credit = money_at(line, "credit")?;
        if debit < zero() || credit < zero() {
            return Err(refused(&format!("line {index} carries a negative amount")));
        }
        let moves_debit = positive(&debit);
        let moves_credit = positive(&credit);
        if moves_debit && moves_credit {
            return Err(refused(&format!(
                "line {index} moves both sides of the entry"
            )));
        }
        if !moves_debit && !moves_credit {
            return Err(refused(&format!("line {index} moves no money")));
        }
        debit_total = debit_total.plus(&debit);
        credit_total = credit_total.plus(&credit);
        lines.push(json!({
            "account_number": text(line, "account_number"),
            "debit": wire(&debit),
            "credit": wire(&credit),
            "description": text(line, "description"),
        }));
    }
    if debit_total != credit_total {
        return Err(refused(&format!(
            "debits and credits differ: {} against {}",
            debit_total.to_fixed(6),
            credit_total.to_fixed(6)
        )));
    }
    Ok(lines)
}

/// The stamped fields of an entry, in the payload shape the kernel hashes.
fn entry_payload(entry_id: &str, entry: &Value) -> Value {
    let lines: Vec<Value> = entry_lines(entry)
        .iter()
        .map(|line| {
            json!({
                "account": field(line, "account_number"),
                "debit": field(line, "debit"),
                "credit": field(line, "credit"),
                "description": field(line, "description"),
            })
        })
        .collect();
    json!({
        "entry_id": entry_id,
        "entry_date": field(entry, "entry_date"),
        "description": field(entry, "description"),
        "reference_number": field(entry, "reference_number"),
        "source_module": field(entry, "source_module"),
        "status": field(entry, "status"),
        "lines": lines,
    })
}

/// The same payload as the service stamped it before this port: amounts as floating-point numbers.
fn legacy_payload(entry_id: &str, entry: &Value) -> Value {
    let floats = |text: &str| text.parse::<f64>().unwrap_or(0.0);
    let lines: Vec<Value> = entry_lines(entry)
        .iter()
        .map(|line| {
            json!({
                "account": field(line, "account_number"),
                "debit": floats(field(line, "debit")),
                "credit": floats(field(line, "credit")),
                "description": field(line, "description"),
            })
        })
        .collect();
    json!({
        "entry_id": entry_id,
        "entry_date": field(entry, "entry_date"),
        "description": field(entry, "description"),
        "reference_number": field(entry, "reference_number"),
        "source_module": field(entry, "source_module"),
        "status": field(entry, "status"),
        "lines": lines,
    })
}

fn in_creation_order(mut rows: Vec<Value>) -> Vec<Value> {
    rows.sort_by(|one, two| {
        field(one, "created_at")
            .cmp(field(two, "created_at"))
            .then(field(one, "id").cmp(field(two, "id")))
    });
    rows
}

fn newest_first(mut rows: Vec<Value>) -> Vec<Value> {
    rows.sort_by(|one, two| {
        field(two, "entry_date")
            .cmp(field(one, "entry_date"))
            .then(field(two, "created_at").cmp(field(one, "created_at")))
    });
    rows
}

/// `POST /journal-entries/`: validated, stamped, and stored as one entry.
pub fn create_entry<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let lines = validated_lines(payload)?;
    // every line must name an account this Book owns
    for line in &lines {
        let number = field(line, "account_number");
        if account_by_number(graph, scope, number)?.is_none() {
            return Err(refused(&format!(
                "Account number {number} not found or does not belong to user."
            )));
        }
    }

    let reference = text(payload, "reference_number");
    let source_module = or_default(payload, "source_module", "Manual");
    if !reference.is_empty() && REFERENCE_CHECKED.contains(&source_module.as_str()) {
        let taken = entries(graph, scope)?.into_iter().any(|entry| {
            field(&entry, "reference_number") == reference
                && field(&entry, "source_module") == source_module
        });
        if taken {
            return Err(refused(&format!(
                "Journal Entry with reference {reference} already exists for source {source_module}."
            )));
        }
    }

    let body = json!({
        "entry_date": text(payload, "entry_date"),
        "description": text(payload, "description"),
        "reference_number": reference,
        "source_module": source_module,
        "status": or_default(payload, "status", "draft"),
        "fraud_flag": or_default(payload, "fraud_flag", "safe"),
        "fraud_score": payload.get("fraud_score").cloned().unwrap_or(json!(0.0)),
        "lines": lines,
        "created_at": text(payload, "created_at"),
        "updated_at": text(payload, "updated_at"),
    });

    let chain = in_creation_order(entries(graph, scope)?);
    let prev_hash = chain
        .last()
        .map(|entry| {
            let stored = field(entry, "entry_hash");
            if stored.is_empty() {
                GENESIS_HASH.to_string()
            } else {
                stored.to_string()
            }
        })
        .unwrap_or_else(|| GENESIS_HASH.to_string());

    // the entry's id is assigned by the store, so the stamp is taken over the body without one: the
    // kernel's payload carries the entry id, and the store's id is stable once written
    let held = body.clone();
    let stamp = compute_entry_hash(&prev_hash, &entry_payload("", &held))
        .map_err(|reason| refused(&format!("cannot stamp the entry: {reason}")))?;

    let mut stored_body = body.clone();
    stored_body["entry_hash"] = json!(stamp);
    stored_body["prev_hash"] = json!(prev_hash);
    let saved = Records::new(graph, JOURNAL_ENTRY).create(scope, &stored_body)?;
    Ok(merge(stored_body, &saved))
}

/// `GET /journal-entries/{id}`.
pub fn get_entry<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    entry_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, JOURNAL_ENTRY).get(scope, entry_id)
}

/// `GET /journal-entries/`, newest business date first.
pub fn list_entries<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let from = optional(query, "start_date");
    let until = optional(query, "end_date");
    let rows: Vec<Value> = entries(graph, scope)?
        .into_iter()
        .filter(|entry| {
            from.as_deref()
                .map(|b| dated_on_or_after(entry, b))
                .unwrap_or(true)
        })
        .filter(|entry| {
            until
                .as_deref()
                .map(|b| dated_on_or_before(entry, b))
                .unwrap_or(true)
        })
        .collect();
    Ok(Value::Array(newest_first(rows)))
}

/// `PUT /journal-entries/{id}`.
///
/// A posted entry is immutable: its stamped fields are what the chain vouches for, so changing one
/// would break its stamp and every stamp after it. Corrections are reversing entries.
pub fn update_entry<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    entry_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let existing = Records::new(graph, JOURNAL_ENTRY).get(scope, entry_id)?;
    if field(&existing, "status") == "posted" {
        return Err(refused(
            "A posted journal entry cannot be edited; correct it with a reversing entry.",
        ));
    }
    let stamped = [
        "entry_date",
        "description",
        "reference_number",
        "source_module",
        "status",
    ];
    if stamped.iter().any(|key| {
        payload
            .get(*key)
            .map(|value| value != existing.get(*key).unwrap_or(&Value::Null))
            .unwrap_or(false)
    }) {
        return Err(refused(
            "A journal entry's stamped fields cannot be edited; correct it with a reversing entry.",
        ));
    }
    Ok(existing)
}

/// `DELETE /journal-entries/{id}`: a draft can go, a posted entry cannot.
pub fn delete_entry<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    entry_id: &str,
) -> Result<Value, StoreError> {
    let existing = Records::new(graph, JOURNAL_ENTRY).get(scope, entry_id)?;
    if field(&existing, "status") == "posted" {
        return Err(refused(
            "A posted journal entry cannot be deleted; correct it with a reversing entry.",
        ));
    }
    Records::new(graph, JOURNAL_ENTRY).delete(scope, entry_id)?;
    Ok(json!({ "ok": true }))
}

/// `GET /ledger/integrity`: walks the chain and reports every entry.
///
/// Each entry is judged under the rule it was written with: the exact decimal payload first, then the
/// older floating-point payload. The whole chain is then walked in order, so a link broken between two
/// entries is still seen.
pub fn verify_chain<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    let chain = in_creation_order(entries(graph, scope)?);
    let mut digests = Vec::new();
    let mut legacy_stamped = Vec::new();

    for entry in &chain {
        let entry_id = field(entry, "id").to_string();
        let stored_hash = field(entry, "entry_hash").to_string();
        let prev_hash = {
            let stored = field(entry, "prev_hash");
            if stored.is_empty() {
                GENESIS_HASH.to_string()
            } else {
                stored.to_string()
            }
        };
        let matches = |payload: &Value| {
            compute_entry_hash(&prev_hash, payload)
                .map(|computed| computed == stored_hash)
                .unwrap_or(false)
        };

        let exact = entry_payload(&entry_id, entry);
        let payload = if matches(&exact) {
            exact
        } else {
            let older = legacy_payload(&entry_id, entry);
            if matches(&older) {
                legacy_stamped.push(entry_id.clone());
            }
            older
        };

        digests.push(EntryDigest {
            entry_id,
            payload: payload.as_object().cloned().unwrap_or_default(),
            prev_hash,
            stored_hash: Some(stored_hash),
        });
    }

    let report = chain_state_from_digests(&digests);
    Ok(json!({
        "valid": report.valid,
        "entries_checked": report.entries_checked,
        "head_hash": report.head_hash,
        "errors": report.errors,
        // entries intact but stamped under the older rendering; they verify, they are just older
        "legacy_stamped": legacy_stamped,
    }))
}

// --- reports over the journal ---

/// `GET /ledgers/{account}`: the account's movements with a running balance.
pub fn ledger_report<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account_number: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let account = require_account(graph, scope, account_number)?;
    let from = optional(query, "start_date");
    let until = optional(query, "end_date");
    let all = entries(graph, scope)?;
    let number = field(&account, "account_number").to_string();

    // everything before the window opens it
    let opening_rows = before(&all, &number, from.as_deref())?;
    let (opening_debit, opening_credit) = totals(&opening_rows);
    let opening = balance_of(&account, &opening_debit, &opening_credit);

    let mut running = opening.clone();
    let mut rows = Vec::new();
    for entry in within(&all, from.as_deref(), until.as_deref()) {
        for line in entry_lines(&entry) {
            if field(&line, "account_number") != number {
                continue;
            }
            let debit = money_at(&line, "debit")?;
            let credit = money_at(&line, "credit")?;
            running = if field(&account, "normal_balance") == "debit" {
                running.plus(&debit).minus(&credit)
            } else {
                running.plus(&credit).minus(&debit)
            };
            rows.push(json!({
                "entry_id": field(&entry, "id"),
                "entry_date": field(&entry, "entry_date"),
                "description": field(&entry, "description"),
                "debit": wire(&debit),
                "credit": wire(&credit),
                "balance": wire(&running),
                "source_module": field(&entry, "source_module"),
            }));
        }
    }

    // the closing balance is the account's balance as of the window's end, which is the running one
    // when the window has no end
    let end_balance = match until.as_deref() {
        Some(bound) => {
            let rows = movements(&all, &number, None, Some(bound))?;
            let (debit, credit) = totals(&rows);
            balance_of(&account, &debit, &credit)
        }
        None => running.clone(),
    };

    Ok(json!({
        "account_number": number,
        "account_name": field(&account, "name"),
        "normal_balance": field(&account, "normal_balance"),
        "start_balance": wire(&opening),
        "entries": rows,
        "end_balance": wire(&end_balance),
    }))
}

/// Movements strictly before a business date, which is what opens a window.
fn before(
    entries: &[Value],
    account_number: &str,
    bound: Option<&str>,
) -> Result<Vec<(Money, Money)>, StoreError> {
    let mut rows = Vec::new();
    for entry in entries.iter().filter(|entry| posted(entry)) {
        if let Some(bound) = bound {
            if field(entry, "entry_date") >= bound {
                continue;
            }
        }
        for line in entry_lines(entry) {
            if field(&line, "account_number") != account_number {
                continue;
            }
            rows.push((money_at(&line, "debit")?, money_at(&line, "credit")?));
        }
    }
    Ok(rows)
}

/// Posted entries inside a window, oldest first.
fn within(entries: &[Value], from: Option<&str>, until: Option<&str>) -> Vec<Value> {
    let mut rows: Vec<Value> = entries
        .iter()
        .filter(|entry| posted(entry))
        .filter(|entry| from.map(|b| dated_on_or_after(entry, b)).unwrap_or(true))
        .filter(|entry| until.map(|b| dated_on_or_before(entry, b)).unwrap_or(true))
        .cloned()
        .collect();
    rows.sort_by(|one, two| {
        field(one, "entry_date")
            .cmp(field(two, "entry_date"))
            .then(field(one, "created_at").cmp(field(two, "created_at")))
    });
    rows
}

/// `GET /trial-balance/`: every account's balance, placed in the column its side calls for.
pub fn trial_balance<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let as_of = optional(query, "as_of_date");
    let all = entries(graph, scope)?;
    let mut rows = Vec::new();
    let mut total_debits = zero();
    let mut total_credits = zero();

    for account in accounts(graph, scope)? {
        let number = field(&account, "account_number").to_string();
        let window = movements(&all, &number, None, as_of.as_deref())?;
        let (debit, credit) = totals(&window);
        let balance = balance_of(&account, &debit, &credit);

        // a balance running against the account's normal side belongs in the other column
        let debit_normal = field(&account, "normal_balance") == "debit";
        let non_negative = balance >= zero();
        let magnitude = if non_negative {
            balance.clone()
        } else {
            zero().minus(&balance)
        };
        let (in_debit, in_credit) = match (debit_normal, non_negative) {
            (true, true) => (balance.clone(), zero()),
            // a debit account carrying a credit balance, or the reverse of it
            (true, false) => (zero(), magnitude),
            (false, true) => (zero(), balance.clone()),
            (false, false) => (magnitude, zero()),
        };

        total_debits = total_debits.plus(&in_debit);
        total_credits = total_credits.plus(&in_credit);
        rows.push(json!({
            "account_number": number,
            "account_name": field(&account, "name"),
            "account_type": field(&account, "account_type"),
            "debit": wire(&in_debit),
            "credit": wire(&in_credit),
        }));
    }

    Ok(json!({
        "report_date": as_of.unwrap_or_default(),
        "accounts": rows,
        "total_debits": wire(&total_debits),
        "total_credits": wire(&total_credits),
        "is_balanced": total_debits == total_credits,
    }))
}

fn accounts_of_type<'a>(rows: &'a [Value], account_type: &str) -> Vec<&'a Value> {
    rows.iter()
        .filter(|row| field(row, "account_type") == account_type)
        .collect()
}

/// `GET /income-statement/`: the period's revenue and expense activity.
pub fn income_statement<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let from = optional(query, "start_date").ok_or_else(|| refused("start_date is required"))?;
    let until = optional(query, "end_date").ok_or_else(|| refused("end_date is required"))?;
    let all = entries(graph, scope)?;
    let chart = accounts(graph, scope)?;

    let mut revenues = Vec::new();
    let mut expenses = Vec::new();
    let mut total_revenue = zero();
    let mut total_expense = zero();

    for account in accounts_of_type(&chart, "revenue") {
        let number = field(account, "account_number").to_string();
        let (debit, credit) = totals(&movements(&all, &number, Some(&from), Some(&until))?);
        let net = credit.minus(&debit);
        // an account whose period runs against its natural side is left out entirely
        if positive(&net) {
            total_revenue = total_revenue.plus(&net);
            revenues.push(json!({"category": field(account, "name"), "amount": wire(&net)}));
        }
    }

    for account in accounts_of_type(&chart, "expense") {
        let number = field(account, "account_number").to_string();
        let (debit, credit) = totals(&movements(&all, &number, Some(&from), Some(&until))?);
        let net = debit.minus(&credit);
        if positive(&net) {
            total_expense = total_expense.plus(&net);
            expenses.push(json!({"category": field(account, "name"), "amount": wire(&net)}));
        }
    }

    Ok(json!({
        "start_date": from,
        "end_date": until,
        "revenues": revenues,
        "expenses": expenses,
        "net_income": wire(&total_revenue.minus(&total_expense)),
    }))
}

/// `GET /balance-sheet/`: every asset, liability and equity account as of a date.
pub fn balance_sheet<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let as_of = optional(query, "as_of_date").ok_or_else(|| refused("as_of_date is required"))?;
    let all = entries(graph, scope)?;
    let chart = accounts(graph, scope)?;

    let lines = |account_type: &str| -> Result<(Vec<Value>, Money), StoreError> {
        let mut rows = Vec::new();
        let mut total = zero();
        for account in accounts_of_type(&chart, account_type) {
            let number = field(account, "account_number").to_string();
            let window = movements(&all, &number, None, Some(&as_of))?;
            let (debit, credit) = totals(&window);
            let balance = balance_of(account, &debit, &credit);
            total = total.plus(&balance);
            rows.push(json!({"category": field(account, "name"), "amount": wire(&balance)}));
        }
        Ok((rows, total))
    };

    let (assets, total_assets) = lines("asset")?;
    let (liabilities, total_liabilities) = lines("liability")?;
    let (equity, total_equity) = lines("equity")?;

    Ok(json!({
        "as_of_date": as_of,
        "assets": assets,
        "liabilities": liabilities,
        "equity": equity,
        "total_assets": wire(&total_assets),
        "total_liabilities": wire(&total_liabilities),
        "total_equity": wire(&total_equity),
        "total_liabilities_equity": wire(&total_liabilities.plus(&total_equity)),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn account(number: &str, kind: &str, normal: &str) -> Value {
        json!({"id": format!("acct-{number}"), "account_number": number,
               "name": format!("{kind} {number}"), "account_type": kind,
               "normal_balance": normal})
    }

    fn entry(status: &str, lines: Value) -> Value {
        json!({"id": "e-1", "entry_date": "2026-03-15", "status": status,
               "description": "sale", "source_module": "Manual", "created_at": "2026-03-15T10:00:00",
               "lines": lines})
    }

    #[test]
    fn a_journal_entry_balances_to_the_last_digit() {
        let lines = validated_lines(&json!({"lines": [
            {"account_number": "1000", "debit": "123456789012345678901.55", "credit": "0"},
            {"account_number": "4000", "debit": "0", "credit": "123456789012345678901.55"}
        ]}))
        .expect("balanced");
        assert_eq!(lines.len(), 2);
        assert_eq!(field(&lines[0], "debit"), "123456789012345678901.550000");
    }

    #[test]
    fn an_unbalanced_entry_is_refused_with_both_totals_in_the_reason() {
        let error = validated_lines(&json!({"lines": [
            {"account_number": "1000", "debit": "10.00"},
            {"account_number": "4000", "credit": "10.01"}
        ]}))
        .expect_err("unbalanced");
        let message = format!("{error}");
        assert!(message.contains("10.000000"), "{message}");
        assert!(message.contains("10.010000"), "{message}");
    }

    #[test]
    fn a_line_may_not_move_both_sides_or_nothing() {
        for lines in [
            json!([{"account_number": "1000", "debit": "1.00", "credit": "1.00"},
                   {"account_number": "4000", "credit": "1.00"}]),
            json!([{"account_number": "1000", "debit": "0", "credit": "0"},
                   {"account_number": "4000", "credit": "1.00"}]),
            json!([{"account_number": "1000", "debit": "-1.00"},
                   {"account_number": "4000", "credit": "1.00"}]),
            json!([{"account_number": "1000", "debit": "1.00"}]),
        ] {
            assert!(validated_lines(&json!({ "lines": lines })).is_err());
        }
    }

    #[test]
    fn a_balance_counts_only_posted_entries() {
        // account_balance reads the journal first, then answers from it
        let graph = FakeGraph::returning(vec![
            entry(
                "posted",
                json!([{"account_number": "1000", "debit": "10.00", "credit": "0"}]),
            ),
            entry(
                "draft",
                json!([{"account_number": "1000", "debit": "999.00", "credit": "0"}]),
            ),
        ]);
        let account = account("1000", "asset", "debit");
        let balance = account_balance(&graph, &scope(), &account, None).expect("balance");
        assert_eq!(balance, Money::parse("10.00").expect("money"));
    }

    #[test]
    fn a_trial_balance_puts_a_negative_balance_in_the_other_column() {
        let graph = FakeGraph::queue(vec![
            // the journal, then the chart
            vec![entry(
                "posted",
                json!([
                    {"account_number": "1000", "debit": "0", "credit": "25.50"},
                    {"account_number": "4000", "debit": "0", "credit": "25.50"},
                ]),
            )],
            vec![
                account("1000", "asset", "debit"),
                account("4000", "revenue", "credit"),
            ],
        ]);
        // the asset account was credited, so it carries a credit balance
        let answer = trial_balance(&graph, &scope(), &json!({})).expect("trial balance");
        assert_eq!(answer["accounts"][0]["debit"], json!("0.000000"));
        assert_eq!(answer["accounts"][0]["credit"], json!("25.500000"));
        assert_eq!(answer["accounts"][1]["credit"], json!("25.500000"));
        // an asset credited and revenue credited cannot balance: both land in the credit column
        assert_eq!(answer["is_balanced"], json!(false));
        assert_eq!(answer["total_credits"], json!("51.000000"));
    }

    #[test]
    fn the_income_statement_reads_the_period_and_hides_accounts_running_the_other_way() {
        let graph = FakeGraph::queue(vec![
            vec![
                entry(
                    "posted",
                    json!([
                        {"account_number": "4000", "debit": "0", "credit": "1000.00"},
                        {"account_number": "5000", "debit": "400.00", "credit": "0"},
                    ]),
                ),
                // a refund inside the same window pulls revenue back below zero
                entry(
                    "posted",
                    json!([
                        {"account_number": "4000", "debit": "1500.00", "credit": "0"},
                        {"account_number": "5000", "debit": "0", "credit": "1500.00"},
                    ]),
                ),
            ],
            vec![
                account("4000", "revenue", "credit"),
                account("5000", "expense", "debit"),
            ],
        ]);
        let answer = income_statement(
            &graph,
            &scope(),
            &json!({"start_date": "2026-03-01", "end_date": "2026-03-31"}),
        )
        .expect("income statement");
        // revenue nets to -500 and is left out; expenses net to -1100 and are left out too
        assert_eq!(answer["revenues"], json!([]));
        assert_eq!(answer["expenses"], json!([]));
        assert_eq!(answer["net_income"], json!("0.000000"));
    }

    #[test]
    fn the_balance_sheet_lists_every_account_of_its_three_types() {
        let graph = FakeGraph::queue(vec![
            vec![entry(
                "posted",
                json!([
                    {"account_number": "1000", "debit": "500.00", "credit": "0"},
                    {"account_number": "3000", "debit": "0", "credit": "500.00"},
                ]),
            )],
            vec![
                account("1000", "asset", "debit"),
                account("2000", "liability", "credit"),
                account("3000", "equity", "credit"),
            ],
        ]);
        let answer =
            balance_sheet(&graph, &scope(), &json!({"as_of_date": "2026-03-31"})).expect("sheet");
        assert_eq!(answer["total_assets"], json!("500.000000"));
        assert_eq!(answer["total_equity"], json!("500.000000"));
        // the liability account never moved, but it is still listed at nothing
        assert_eq!(answer["liabilities"][0]["amount"], json!("0.000000"));
        assert_eq!(answer["total_liabilities_equity"], json!("500.000000"));
    }

    #[test]
    fn a_ledger_opens_with_what_happened_before_its_window() {
        let graph = FakeGraph::queue(vec![
            vec![account("1000", "asset", "debit")],
            vec![
                json!({"id": "e-1", "entry_date": "2026-02-20", "status": "posted",
                       "created_at": "2026-02-20T09:00:00",
                       "lines": [{"account_number": "1000", "debit": "100.00", "credit": "0"}]}),
                json!({"id": "e-2", "entry_date": "2026-03-05", "status": "posted",
                       "created_at": "2026-03-05T09:00:00",
                       "lines": [{"account_number": "1000", "debit": "0", "credit": "30.00"}]}),
                json!({"id": "e-3", "entry_date": "2026-03-20", "status": "posted",
                       "created_at": "2026-03-20T09:00:00",
                       "lines": [{"account_number": "1000", "debit": "5.00", "credit": "0"}]}),
            ],
        ]);
        let answer = ledger_report(
            &graph,
            &scope(),
            "1000",
            &json!({"start_date": "2026-03-01", "end_date": "2026-03-31"}),
        )
        .expect("ledger");
        assert_eq!(answer["start_balance"], json!("100.000000"));
        assert_eq!(answer["entries"].as_array().expect("rows").len(), 2);
        assert_eq!(answer["entries"][0]["balance"], json!("70.000000"));
        assert_eq!(answer["entries"][1]["balance"], json!("75.000000"));
        assert_eq!(answer["end_balance"], json!("75.000000"));
    }

    #[test]
    fn an_account_holding_journal_lines_cannot_be_deleted() {
        let graph = FakeGraph::queue(vec![
            vec![account("1000", "asset", "debit")],
            vec![entry(
                "posted",
                json!([{"account_number": "1000", "debit": "1.00", "credit": "0"}]),
            )],
        ]);
        assert!(delete_account(&graph, &scope(), "1000").is_err());
        // nothing was deleted: the two reads were the whole conversation
        assert_eq!(graph.statements(), 2);
    }

    #[test]
    fn a_created_account_answers_with_the_fields_it_was_written_from() {
        let graph = FakeGraph::queue(vec![vec![], vec![json!({"id": "a-1"})]]);
        let answer = create_account(
            &graph,
            &scope(),
            &json!({"account_number": "1000", "name": "Cash", "account_type": "asset",
                    "normal_balance": "debit"}),
        )
        .expect("created");
        assert_eq!(answer["account_number"], json!("1000"));
        assert_eq!(answer["account_type"], json!("asset"));
        assert_eq!(answer["id"], json!("a-1"));
    }

    #[test]
    fn an_account_number_cannot_be_taken_twice() {
        let graph = FakeGraph::returning(vec![account("1000", "asset", "debit")]);
        assert!(create_account(
            &graph,
            &scope(),
            &json!({"account_number": "1000", "name": "Cash"})
        )
        .is_err());
    }

    #[test]
    fn a_posted_entry_cannot_be_edited_or_deleted() {
        let graph = FakeGraph::returning(vec![entry(
            "posted",
            json!([{"account_number": "1000", "debit": "1.00", "credit": "0"}]),
        )]);
        let edited = update_entry(&graph, &scope(), "e-1", &json!({"description": "changed"}));
        assert!(edited.is_err(), "a posted entry is immutable");
        assert!(delete_entry(&graph, &scope(), "e-1").is_err());
    }

    #[test]
    fn a_draft_entry_can_be_deleted() {
        let graph = FakeGraph::queue(vec![
            vec![entry(
                "draft",
                json!([{"account_number": "1000", "debit": "1.00", "credit": "0"}]),
            )],
            vec![json!({"id": "e-1"})],
        ]);
        let answer = delete_entry(&graph, &scope(), "e-1").expect("deleted");
        assert_eq!(answer["ok"], json!(true));
    }

    #[test]
    fn an_entry_is_stamped_over_the_stamp_before_it() {
        let graph = FakeGraph::queue(vec![
            // the chart, once per line, then the chain, then the write
            vec![
                account("1000", "asset", "debit"),
                account("4000", "revenue", "credit"),
            ],
            vec![
                account("1000", "asset", "debit"),
                account("4000", "revenue", "credit"),
            ],
            vec![json!({"id": "e-9", "created_at": "2026-01-01T00:00:00", "entry_hash": "abc"})],
            vec![json!({"id": "e-1"})],
        ]);
        let answer = create_entry(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-03-15", "status": "posted",
            "lines": [
                {"account_number": "1000", "debit": "10.00"},
                {"account_number": "4000", "credit": "10.00"}
            ]}),
        )
        .expect("entry");
        assert_eq!(answer["prev_hash"], json!("abc"));
        let stamp = answer["entry_hash"].as_str().expect("stamp");
        assert_eq!(stamp.len(), 64);
    }

    #[test]
    fn a_chain_of_one_entry_verifies_against_the_genesis_stamp() {
        let mut body = json!({"id": "e-1", "entry_date": "2026-03-15", "description": "sale",
                              "reference_number": "", "source_module": "Manual", "status": "posted",
                              "created_at": "2026-03-15T10:00:00",
                              "lines": [{"account_number": "1000", "debit": "10.000000",
                                         "credit": "0.000000", "description": ""}]});
        let payload = entry_payload("e-1", &body);
        let stamp = compute_entry_hash(GENESIS_HASH, &payload).expect("stamp");
        body["entry_hash"] = json!(stamp);
        body["prev_hash"] = json!(GENESIS_HASH);

        let graph = FakeGraph::returning(vec![body]);
        let answer = verify_chain(&graph, &scope()).expect("integrity");
        assert_eq!(answer["valid"], json!(true));
        assert_eq!(answer["entries_checked"], json!(1));
        assert_eq!(answer["legacy_stamped"], json!([]));
    }

    #[test]
    fn an_entry_stamped_the_old_way_still_verifies() {
        let mut body = json!({"id": "e-1", "entry_date": "2026-03-15", "description": "sale",
                              "reference_number": "", "source_module": "Manual", "status": "posted",
                              "created_at": "2026-03-15T10:00:00",
                              "lines": [{"account_number": "1000", "debit": "10.000000",
                                         "credit": "0.000000", "description": ""}]});
        // the pre-port rendering: amounts as floating-point numbers
        let stamp = compute_entry_hash(GENESIS_HASH, &legacy_payload("e-1", &body)).expect("stamp");
        body["entry_hash"] = json!(stamp);
        body["prev_hash"] = json!(GENESIS_HASH);

        let graph = FakeGraph::returning(vec![body]);
        let answer = verify_chain(&graph, &scope()).expect("integrity");
        assert_eq!(answer["valid"], json!(true));
        assert_eq!(answer["legacy_stamped"], json!(["e-1"]));
    }

    #[test]
    fn a_retroactive_change_breaks_the_chain() {
        let mut first = json!({"id": "e-1", "entry_date": "2026-03-15", "description": "sale",
                               "reference_number": "", "source_module": "Manual", "status": "posted",
                               "created_at": "2026-03-15T10:00:00",
                               "lines": [{"account_number": "1000", "debit": "10.000000",
                                          "credit": "0.000000", "description": ""}]});
        let stamp = compute_entry_hash(GENESIS_HASH, &entry_payload("e-1", &first)).expect("stamp");
        first["entry_hash"] = json!(stamp);
        first["prev_hash"] = json!(GENESIS_HASH);
        // the amount is changed under the stored stamp
        first["lines"][0]["debit"] = json!("99.000000");

        let graph = FakeGraph::returning(vec![first]);
        let answer = verify_chain(&graph, &scope()).expect("integrity");
        assert_eq!(answer["valid"], json!(false));
        assert!(!answer["errors"].as_array().expect("errors").is_empty());
    }

    #[test]
    fn a_reference_number_is_unique_only_for_the_modules_the_service_checks() {
        let graph = FakeGraph::queue(vec![
            vec![
                account("1000", "asset", "debit"),
                account("4000", "revenue", "credit"),
            ],
            vec![
                account("1000", "asset", "debit"),
                account("4000", "revenue", "credit"),
            ],
            vec![
                json!({"id": "e-9", "reference_number": "INV-1", "source_module": "Invoicing",
                        "created_at": "2026-01-01T00:00:00", "entry_hash": "abc"}),
            ],
        ]);
        let clash = create_entry(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-03-15", "reference_number": "INV-1",
                    "source_module": "Invoicing",
                    "lines": [{"account_number": "1000", "debit": "10.00"},
                              {"account_number": "4000", "credit": "10.00"}]}),
        );
        assert!(clash.is_err(), "the reference is taken for Invoicing");

        // a module outside the checked set may reuse it
        let graph = FakeGraph::queue(vec![
            vec![
                account("1000", "asset", "debit"),
                account("4000", "revenue", "credit"),
            ],
            vec![
                account("1000", "asset", "debit"),
                account("4000", "revenue", "credit"),
            ],
            vec![
                json!({"id": "e-9", "reference_number": "INV-1", "source_module": "Invoicing",
                        "created_at": "2026-01-01T00:00:00", "entry_hash": "abc"}),
            ],
            vec![json!({"id": "e-1"})],
        ]);
        create_entry(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-03-15", "reference_number": "INV-1",
                    "source_module": "Payroll",
                    "lines": [{"account_number": "1000", "debit": "10.00"},
                              {"account_number": "4000", "credit": "10.00"}]}),
        )
        .expect("a module outside the checked set");
    }

    #[test]
    fn an_entry_naming_an_account_the_book_does_not_own_is_refused() {
        let graph = FakeGraph::returning(vec![account("1000", "asset", "debit")]);
        let refused = create_entry(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-03-15",
                    "lines": [{"account_number": "1000", "debit": "10.00"},
                              {"account_number": "9999", "credit": "10.00"}]}),
        );
        assert!(refused.is_err());
    }
}
