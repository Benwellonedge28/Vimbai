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

use std::collections::BTreeMap;

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_ledger_core::hash_chain::{
    chain_state_from_digests, compute_entry_hash, EntryDigest, GENESIS_HASH,
};
use vimbai_store::amounts::wire;
use vimbai_store::read::money_or_nothing;
use vimbai_store::read::{field, flag, optional, or_default, refused, text};
use vimbai_store::{
    created_record, BookScope, Graph, Records, StoreError, CASH_BANK_ACCOUNT, JOURNAL_DRAFT,
    JOURNAL_DRAFT_LINE, JOURNAL_ENTRY, JOURNAL_LINE, LEDGER_ACCOUNT,
};

/// The source modules whose reference numbers the service keeps unique.
/// The module that closes a year. Its entries are not trading: they move a year's profit to equity, so no report of what
/// an account did should count them as though the business had traded again.
pub const CLOSING_ENTRY_MODULE: &str = "YearEndClose";

const REFERENCE_CHECKED: [&str; 16] = [
    "Banking",
    // a month's depreciation is computed, not typed, so its reference is the asset and the month: a second run of the same
    // month would double a charge that the books have already taken
    "Depreciation",
    // an expense is typed from a receipt by a person, like a manual entry, so its reference has to be unique or the same
    // cost can be recorded twice without anything noticing
    "ExpensesJournal",
    // an estimate is one per cost per month and a prepayment's month is released once, so the reference says which cost
    // and which month: a second press must be refused rather than charging or spending the same month twice
    "AccrualsJournal",
    "PrepaymentsJournal",
    // a month's wages are worked out once and paid once, so its reference is the payroll's own: a second press must be
    // refused rather than charging the same month's wages twice and owing the staff twice
    "Payroll",
    // a Book has one first day, so the opening set is stated once: a second set must be refused rather than quietly
    // moving the starting point every later month is measured from
    "OpeningBalances",
    // giving up on a debt is a decision with a day: the reference names the debtor and the day, so a second press cannot
    // take the same debt out of the books twice
    "BadDebts",
    // a provision is adjusted once a day: the reference names the day, so a second press cannot charge the month twice
    "DoubtfulDebts",
    // a count is a document with a day: the reference names the thing counted and the day, so a second press cannot take
    // the same shortage out of the books twice
    "StockAdjustments",
    // a year is closed once: the reference is the year, so a second press cannot move the year's profit to equity twice
    CLOSING_ENTRY_MODULE,
    "Invoicing",
    "Multimodal",
    "SupplyChain",
    "Manual",
    "MobileApp",
];

fn zero() -> Money {
    Money::zero()
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

pub fn accounts<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Vec<Value>, StoreError> {
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
    for entry in Records::new(graph, JOURNAL_ENTRY).list_with_children(scope, 2000, JOURNAL_LINE)? {
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
    // the lines are records of their own now, so they are read with the entry rather than out of it
    Records::new(graph, JOURNAL_ENTRY).list_with_children(scope, 4000, JOURNAL_LINE)
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
            rows.push((
                money_or_nothing(&line, "debit")?,
                money_or_nothing(&line, "credit")?,
            ));
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

/// What every account has taken up between two dates, signed in the account's own direction.
///
/// This is the one place that decides what "spent" means for a period, so a report comparing a plan against what actually
/// happened reads it here rather than adding journal lines up again. Only posted entries count: a draft moves no money, so
/// counting one would report spending that has not happened. An account with no posted movement is left out rather than
/// listed at nothing, because "no movement" and "moved nothing" are the same answer and the caller already knows both.
///
/// The sign follows the account: a debit-normal account (an expense, an asset) reports debits minus credits, so spending is
/// positive. A credit-normal account (revenue, a liability) reports credits minus debits, so what was earned or raised is
/// positive. Without that, a plan for sales would read as negative earnings.
pub fn posted_activity<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    from: &str,
    until: &str,
) -> Result<BTreeMap<String, Money>, StoreError> {
    let mut moved: BTreeMap<String, (Money, Money)> = BTreeMap::new();
    for entry in entries(graph, scope)?.iter().filter(|entry| posted(entry)) {
        if !dated_on_or_after(entry, from) || !dated_on_or_before(entry, until) {
            continue;
        }
        // closing a year is not trading: its entry empties the year's revenue and cost accounts, so counting it would
        // report a year that earned nothing because it had just been closed
        if field(entry, "source_module") == CLOSING_ENTRY_MODULE {
            continue;
        }
        for line in entry_lines(entry) {
            let account_number = field(&line, "account_number");
            if account_number.is_empty() {
                continue;
            }
            let slot = moved
                .entry(account_number.to_string())
                .or_insert_with(|| (zero(), zero()));
            slot.0 = slot.0.plus(&money_or_nothing(&line, "debit")?);
            slot.1 = slot.1.plus(&money_or_nothing(&line, "credit")?);
        }
    }

    let chart = accounts(graph, scope)?;
    let mut activity: BTreeMap<String, Money> = BTreeMap::new();
    for (account_number, (debit, credit)) in moved {
        let account = chart
            .iter()
            .find(|row| field(row, "account_number") == account_number);
        let signed = match account {
            Some(account) => balance_of(account, &debit, &credit),
            // an account the chart no longer holds: report the debit side, which is what a spending line would mean
            None => debit.minus(&credit),
        };
        if !signed.is_zero() {
            activity.insert(account_number, signed);
        }
    }
    Ok(activity)
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
        let debit = money_or_nothing(line, "debit")?;
        let credit = money_or_nothing(line, "credit")?;
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
            // said outright rather than inferred: the store links the line to its account by this side
            "side": if moves_debit { "debit" } else { "credit" },
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
    let mut payload = json!({
        "entry_id": entry_id,
        "entry_date": field(entry, "entry_date"),
        "description": field(entry, "description"),
        "reference_number": field(entry, "reference_number"),
        "source_module": field(entry, "source_module"),
        "status": field(entry, "status"),
        "lines": lines,
    });
    // which entry this one corrects is part of what it says, so it is inside the stamp. Only added when there is one:
    // an entry posted before corrections existed was stamped over a payload without this key, and adding it there
    // would fail every one of those hashes for a fact they never carried.
    let corrects = field(entry, "reverses_entry_id");
    if !corrects.is_empty() {
        if let Some(object) = payload.as_object_mut() {
            object.insert("reverses_entry_id".into(), json!(corrects));
        }
    }
    payload
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

/// The entries in the order they were written, which is the order the chain was stamped in.
///
/// A record's own timestamp is kept to the second, so two entries posted inside one second carry the same one and only
/// the id is left to separate them. That works because an id is minted with the moment and a counter in its leading
/// digits: it ascends with the order the records were written, on one device and across restarts. An id that were
/// merely unique would put two entries of one second in an order of its own choosing, and a chain that is whole would
/// be reported as broken.
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
    // a draft is a note, not a posting: it is kept outside the journal until somebody posts it
    if or_default(payload, "status", "draft") == "draft" {
        return save_draft(graph, scope, payload);
    }
    write_posting(graph, scope, payload, None)
}

/// Write the posting itself. `reusing` is the id the entry will carry, which only the post-a-note path supplies: the note
/// and the posting it becomes are the same thing at the same address, so following a link to the note still works after
/// it is posted. A client naming its own id is not honoured, because an entry's id is not a field it gets to choose.
fn write_posting<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
    reusing: Option<&str>,
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

    // a month that has been reported on and closed cannot be written into: last month's figures have been read, filed
    // and acted on, and an entry arriving now with an old date would change them without anybody being told
    let entry_date = text(payload, "entry_date");
    if let Some(refusal) = super::periods::refusal_for(graph, scope, &entry_date)? {
        return Err(refused(&refusal));
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

    // the id comes first, because the stamp has to be taken over the id the entry will actually carry
    let entry_id = reusing
        .map(str::to_owned)
        .unwrap_or_else(|| vimbai_store::minted_id(&scope.book_id));
    let body = json!({
        "id": entry_id,
        "entry_date": entry_date,
        "description": text(payload, "description"),
        "reference_number": reference,
        "source_module": source_module,
        "status": or_default(payload, "status", "draft"),
        "fraud_flag": or_default(payload, "fraud_flag", "safe"),
        "fraud_score": payload.get("fraud_score").cloned().unwrap_or(json!(0.0)),
        "created_at": text(payload, "created_at"),
        "updated_at": text(payload, "updated_at"),
    });

    let mut body = body;
    let corrects = text(payload, "reverses_entry_id");
    if !corrects.is_empty() {
        body["reverses_entry_id"] = json!(corrects);
    }

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

    // the lines are children of the entry, not fields on it, so they are put back for the stamp: the chain has to vouch
    // for what the entry says, and what it says is its lines
    let mut stamping = body.clone();
    if let Some(object) = stamping.as_object_mut() {
        object.insert("lines".into(), Value::Array(lines.clone()));
    }
    let stamp = compute_entry_hash(&prev_hash, &entry_payload(&entry_id, &stamping))
        .map_err(|reason| refused(&format!("cannot stamp the entry: {reason}")))?;

    let mut stored_body = body.clone();
    stored_body["entry_hash"] = json!(stamp);
    stored_body["prev_hash"] = json!(prev_hash);
    // each line is stamped with the entry it belongs to, so it can be found by account without reading the entries
    let mut held: Vec<Value> = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let mut line = line.clone();
        if let Some(object) = line.as_object_mut() {
            object.insert("entry_id".into(), json!(entry_id));
            object.insert("line_no".into(), json!(index + 1));
        }
        held.push(line);
    }
    let mut body_without_lines = stored_body.clone();
    if let Some(object) = body_without_lines.as_object_mut() {
        object.remove("lines");
    }
    let saved = Records::new(graph, JOURNAL_ENTRY).create_with_children(
        scope,
        &body_without_lines,
        JOURNAL_LINE,
        &held,
        "entry_id",
    )?;
    let mut answer = merge(stored_body, &saved);
    if let Some(object) = answer.as_object_mut() {
        object.insert("lines".into(), Value::Array(lines));
    }
    Ok(answer)
}

// --- drafts: the notes that are not postings ---

/// The drafts in this Book, each with its lines, newest business date first.
fn drafts<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, JOURNAL_DRAFT).list_with_children(scope, 4000, JOURNAL_DRAFT_LINE)
}

fn find_draft<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    draft_id: &str,
) -> Result<Option<Value>, StoreError> {
    Ok(drafts(graph, scope)?
        .into_iter()
        .find(|draft| field(draft, "id") == draft_id))
}

/// `POST /journal-entries/` with a status of `draft`: validated exactly as a posting is, then kept as a note.
///
/// The same rules apply because a draft that could not be posted later would be a record of nothing. What it does not
/// get is a place in the journal: no hash, no link, nothing for the chain to vouch for, which is what lets it be edited
/// and thrown away without damaging the books.
fn save_draft<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let lines = validated_lines(payload)?;
    for line in &lines {
        let number = field(line, "account_number");
        if account_by_number(graph, scope, number)?.is_none() {
            return Err(refused(&format!(
                "Account number {number} not found or does not belong to user."
            )));
        }
    }
    let reference = text(payload, "reference_number");
    let body = json!({
        "entry_date": text(payload, "entry_date"),
        "description": text(payload, "description"),
        "reference_number": reference,
        "source_module": or_default(payload, "source_module", "Manual"),
        "status": "draft",
        "created_at": text(payload, "created_at"),
        "updated_at": text(payload, "updated_at"),
    });
    let held: Vec<Value> = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let mut line = line.clone();
            if let Some(object) = line.as_object_mut() {
                object.insert("line_no".into(), json!(index + 1));
            }
            line
        })
        .collect();
    let saved = Records::new(graph, JOURNAL_DRAFT).create_with_children(
        scope,
        &body,
        JOURNAL_DRAFT_LINE,
        &held,
        "draft_id",
    )?;
    let mut answer = merge(body, &saved);
    if let Some(object) = answer.as_object_mut() {
        object.insert("lines".into(), Value::Array(lines));
    }
    Ok(answer)
}

/// `PUT /journal-entries/{id}` on a note: the header may change, the lines may not.
///
/// Rewriting a note's lines would be rewriting the note, and the honest way to do that is to throw it away and write it
/// again. What a person does need is to fix the date, the description or the reference of work still in progress.
fn edit_draft<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    draft_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let Some(draft) = find_draft(graph, scope, draft_id)? else {
        return Err(refused(&format!(
            "No journal entry {draft_id} in this Book."
        )));
    };
    if let Some(lines) = payload.get("lines").and_then(Value::as_array) {
        if !lines.is_empty() {
            return Err(refused(
                "A draft's lines are not edited in place; delete the draft and write it again.",
            ));
        }
    }
    let mut fields = serde_json::Map::new();
    for key in [
        "entry_date",
        "description",
        "reference_number",
        "source_module",
    ] {
        if let Some(value) = payload.get(key) {
            if value != draft.get(key).unwrap_or(&Value::Null) {
                fields.insert(key.into(), value.clone());
            }
        }
    }
    if fields.is_empty() {
        return Ok(draft);
    }
    let edited =
        Records::new(graph, JOURNAL_DRAFT).update(scope, draft_id, &Value::Object(fields))?;
    let mut answer = merge(draft, &edited);
    if let Some(object) = answer.as_object_mut() {
        object.insert("status".into(), json!("draft"));
    }
    Ok(answer)
}

/// `POST /journal-entries/{id}/post`: turn a note into the posting it describes.
///
/// The posting is written by the ordinary door, so it is validated and stamped like any other entry, and only then is
/// the note removed: a draft that failed to post stays exactly where it was rather than being lost.
pub fn post_draft<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    draft_id: &str,
) -> Result<Value, StoreError> {
    let Some(draft) = find_draft(graph, scope, draft_id)? else {
        if Records::new(graph, JOURNAL_ENTRY)
            .get(scope, draft_id)
            .is_ok()
        {
            return Err(refused(
                "That entry is already posted; a posted entry is not edited, it is corrected with a reversing entry.",
            ));
        }
        return Err(refused(&format!("No draft {draft_id} in this Book.")));
    };
    let posting = json!({
        "entry_date": field(&draft, "entry_date"),
        "description": field(&draft, "description"),
        "reference_number": field(&draft, "reference_number"),
        "source_module": field(&draft, "source_module"),
        "status": "posted",
        "created_at": field(&draft, "created_at"),
        "lines": entry_lines(&draft),
    });
    let posted = write_posting(graph, scope, &posting, Some(draft_id))?;
    Records::new(graph, JOURNAL_DRAFT).delete(scope, draft_id)?;
    Ok(posted)
}

/// `GET /accounts/{number}/postings`: the lines that moved one account, fetched through the ledger's own edges.
///
/// This is the read the `DEBITED`/`CREDITED` edges exist for. Answering it by listing every entry in the Book and
/// filtering would return the same rows today and get slower with every entry posted; walking in from the account does
/// not, which is the difference between having lineage and having an index.
pub fn account_postings<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account_number: &str,
    limit: usize,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, JOURNAL_ENTRY).lines_for_account(
        scope,
        JOURNAL_LINE,
        account_number,
        limit,
    )?;
    let mut postings: Vec<Value> = Vec::new();
    let mut debits = zero();
    let mut credits = zero();
    for row in &rows {
        let line = row.get("l").cloned().unwrap_or(Value::Null);
        let debit = money_or_nothing(&line, "debit")?;
        let credit = money_or_nothing(&line, "credit")?;
        debits = debits.plus(&debit);
        credits = credits.plus(&credit);
        postings.push(json!({
            "entry_id": row.get("entry_id").cloned().unwrap_or(Value::Null),
            "entry_date": row.get("entry_date").cloned().unwrap_or(Value::Null),
            "description": field(&line, "description"),
            "line_no": line.get("line_no").cloned().unwrap_or(Value::Null),
            "debit": wire(&debit),
            "credit": wire(&credit),
        }));
    }
    Ok(json!({
        "account_number": account_number,
        "postings": postings,
        "total_debit": wire(&debits),
        "total_credit": wire(&credits),
    }))
}

/// `GET /journal-entries/{id}`.
pub fn get_entry<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    entry_id: &str,
) -> Result<Value, StoreError> {
    match Records::new(graph, JOURNAL_ENTRY).get_with_children(scope, entry_id, JOURNAL_LINE) {
        Ok(entry) => Ok(entry),
        // a draft is not in the journal, so the same address answers for it out of the notes
        Err(_) => find_draft(graph, scope, entry_id)?
            .ok_or_else(|| refused(&format!("No journal entry {entry_id} in this Book."))),
    }
}

/// `GET /journal-entries/`, newest business date first.
pub fn list_entries<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let from = optional(query, "start_date");
    let until = optional(query, "end_date");
    // the notes come too: a person looking at their entries expects the one they have not posted yet to be there,
    // carrying its own status so the list can say it is not in the statements
    let mut all = entries(graph, scope)?;
    all.extend(drafts(graph, scope)?);
    let rows: Vec<Value> = all
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
    let existing = match Records::new(graph, JOURNAL_ENTRY).get(scope, entry_id) {
        Ok(entry) => entry,
        Err(_) => return edit_draft(graph, scope, entry_id, payload),
    };
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

/// `POST /journal-entries/{id}/reverse`: the correcting entry for one that was already posted.
///
/// A posted entry is never edited, so a mistake is answered with an entry that says the opposite. The engine reads the
/// original's own lines and swaps their sides, copying each amount exactly as it stands: no arithmetic is done on the
/// money, so a correction of a thirty-digit figure cannot come back a unit out, and a balanced original reverses to a
/// balanced correction by construction rather than by re-adding.
///
/// `corrected` becomes the entry's date when the caller gives one, and the original's date otherwise - not today's,
/// because this crate has no clock and a correction silently dated on the machine's day would land in whatever period
/// the machine thinks it is. The correction refers back to its original in `reverses_entry_id`, which is how a second
/// attempt is refused: one mistake has exactly one correction, or the books would drift from the truth by the same
/// amount twice.
pub fn reverse_entry<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    entry_id: &str,
    corrected: Option<&str>,
) -> Result<Value, StoreError> {
    let held = entries(graph, scope)?;
    let Some(original) = held
        .iter()
        .find(|entry| field(entry, "id") == entry_id)
        .cloned()
    else {
        if Records::new(graph, JOURNAL_DRAFT)
            .get(scope, entry_id)
            .is_ok()
        {
            return Err(refused(
                "That entry is a draft: a draft is edited or thrown away, not reversed.",
            ));
        }
        return Err(refused(&format!(
            "No posted journal entry {entry_id} in this Book."
        )));
    };

    if let Some(already) = held
        .iter()
        .find(|entry| field(entry, "reverses_entry_id") == entry_id)
    {
        return Err(refused(&format!(
            "That entry has already been reversed by {}. A correction is made once; reversing it again would count the same mistake twice.",
            field(already, "id")
        )));
    }

    let mut lines = Vec::new();
    for line in entry_lines(&original) {
        // the sides swap and the amounts travel untouched; the stored side is what says which column is the money, so
        // the correction's side is simply the other one and no amount is parsed to find out
        let (debit, credit) = (field(&line, "credit"), field(&line, "debit"));
        let moves_debit = field(&line, "side") == "credit";
        lines.push(json!({
            "account_number": field(&line, "account_number"),
            "debit": debit,
            "credit": credit,
            "description": field(&line, "description"),
            "side": if moves_debit { "debit" } else { "credit" },
        }));
    }

    let description = field(&original, "description");
    let reference = field(&original, "reference_number");
    let posting = json!({
        "entry_date": corrected
            .map(str::to_owned)
            .filter(|date| !date.is_empty())
            .unwrap_or_else(|| field(&original, "entry_date").to_string()),
        "description": if description.is_empty() {
            "Reversal".to_string()
        } else {
            format!("Reversal of {description}")
        },
        // suffixed so the correction cannot collide with the reference the original took; the uniqueness rule only
        // sees the same reference twice within one source module, and this is a different string
        "reference_number": if reference.is_empty() {
            String::new()
        } else {
            format!("{reference}-R")
        },
        "source_module": "Manual",
        "status": "posted",
        "reverses_entry_id": entry_id,
        "lines": lines,
    });
    write_posting(graph, scope, &posting, None)
}

/// `DELETE /journal-entries/{id}`: a draft can go, a posted entry cannot.
pub fn delete_entry<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    entry_id: &str,
) -> Result<Value, StoreError> {
    let existing = match Records::new(graph, JOURNAL_ENTRY).get(scope, entry_id) {
        Ok(entry) => entry,
        Err(_) => {
            // throwing a note away touches no stamp and no link, because a note was never in the chain
            if find_draft(graph, scope, entry_id)?.is_none() {
                return Err(refused(&format!(
                    "No journal entry {entry_id} in this Book."
                )));
            }
            Records::new(graph, JOURNAL_DRAFT).delete(scope, entry_id)?;
            return Ok(json!({ "ok": true }));
        }
    };
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
            let debit = money_or_nothing(&line, "debit")?;
            let credit = money_or_nothing(&line, "credit")?;
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
        // the type is what a person reads above the rows: a ledger screen that only knows the normal side
        // still has to say whether the account it opened is an asset, a liability or revenue
        "account_type": field(&account, "account_type"),
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
            rows.push((
                money_or_nothing(&line, "debit")?,
                money_or_nothing(&line, "credit")?,
            ));
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

/// `GET /cash-flow/`: where the money moved, read from the posted ledger.
///
/// A cash flow statement is not a second set of books: it is the same entries looked at from the cash side. So this
/// reads posted entries only, exactly like the trial balance, the income statement and the balance sheet, and it takes
/// the set of cash accounts from the cash book - each cash or bank account names the chart account it is kept against -
/// rather than guessing from an account number.
///
/// Each entry is classified by what the money moved against, and the classification needs no allocation. An entry
/// balances, so the cash it moved is exactly the negative of everything else it moved; grouping those other lines by
/// their account's type gives each class its share to the last digit, with nothing left over:
///
/// * revenue and expense accounts are operating - trading;
/// * other asset accounts are investing - buying or selling what the business uses;
/// * liability and equity accounts are financing - borrowing, repaying, and what the owners put in or took out.
///
/// An account type outside those five is not swept into operating quietly: it is counted as unclassified so a chart
/// carrying something unusual says so instead of hiding it.
///
/// Opening cash is the ledger's own cash movement before the period, so opening plus the period's movement is closing
/// by construction and a statement can never disagree with itself. The cash book's own total is reported beside it,
/// because the two are kept by different rules (a cash book account can be opened with a balance that was never an
/// entry) and a person should be told when they differ rather than shown one and left to assume.
/// One class of a cash flow statement: its heading, the counterpart accounts that actually moved money through cash,
/// and how many accounts in that class moved nothing (a count, never a row of zeros).
struct FlowClass<'a> {
    title: &'a str,
    sources: Vec<(String, Money)>,
    quiet: usize,
}

impl<'a> FlowClass<'a> {
    fn new(title: &'a str) -> Self {
        Self {
            title,
            sources: Vec::new(),
            quiet: 0,
        }
    }
}

pub fn cash_flow<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let from = optional(query, "start_date").ok_or_else(|| refused("start_date is required"))?;
    let until = optional(query, "end_date").ok_or_else(|| refused("end_date is required"))?;

    // which chart accounts are cash: the cash book's own answer, not a guess at the numbering
    let cash_accounts: Vec<(String, String)> = Records::new(graph, CASH_BANK_ACCOUNT)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| flag(row, "is_active"))
        .map(|row| {
            (
                field(&row, "account_code").to_string(),
                field(&row, "account_name").to_string(),
            )
        })
        .collect();
    if cash_accounts.is_empty() {
        return Err(refused(
            "no cash or bank account is recorded, so there is no cash movement to report",
        ));
    }
    let is_cash = |number: &str| cash_accounts.iter().any(|(code, _)| code == number);

    let all = entries(graph, scope)?;
    let chart = accounts(graph, scope)?;
    let kind_of = |number: &str| -> String {
        chart
            .iter()
            .find(|account| field(account, "account_number") == number)
            .map(|account| field(account, "account_type").to_string())
            .unwrap_or_default()
    };
    let name_of = |number: &str| -> String {
        chart
            .iter()
            .find(|account| field(account, "account_number") == number)
            .map(|account| field(account, "name").to_string())
            .unwrap_or_else(|| number.to_string())
    };

    // where each class's money came from, kept per counterpart account so a person reads sources, not totals
    let mut classes = vec![
        FlowClass::new("Operating"),
        FlowClass::new("Investing"),
        FlowClass::new("Financing"),
    ];
    let mut unclassified = 0usize;
    let mut movement = zero();
    let mut opening = zero();

    for entry in all.iter().filter(|entry| posted(entry)) {
        let mut cash_here = zero();
        let mut by_class: Vec<Vec<(String, Money)>> = vec![Vec::new(), Vec::new(), Vec::new()];
        let mut odd_here = 0usize;
        for line in entry_lines(entry) {
            let number = field(&line, "account_number").to_string();
            let debit = money_or_nothing(&line, "debit")?;
            let credit = money_or_nothing(&line, "credit")?;
            let signed = debit.minus(&credit);
            if is_cash(&number) {
                cash_here = cash_here.plus(&signed);
                continue;
            }
            if is_zero(&signed) {
                continue;
            }
            let slot = match kind_of(&number).as_str() {
                "revenue" | "expense" => Some(0usize),
                "asset" => Some(1),
                "liability" | "equity" => Some(2),
                _ => None,
            };
            match slot {
                Some(index) => by_class[index].push((name_of(&number), signed.negate())),
                None => odd_here += 1,
            }
        }
        if is_zero(&cash_here) {
            continue;
        }
        if !dated_on_or_before(entry, &until) {
            // later than the period: not this statement's business at all
            continue;
        }
        if !dated_on_or_after(entry, &from) {
            // everything before the period is what the business held when it opened
            opening = opening.plus(&cash_here);
            continue;
        }
        movement = movement.plus(&cash_here);
        unclassified += odd_here;
        for (index, class) in classes.iter_mut().enumerate() {
            for (name, amount) in by_class[index].drain(..) {
                if is_zero(&amount) {
                    class.quiet += 1;
                    continue;
                }
                match class.sources.iter_mut().find(|(known, _)| *known == name) {
                    Some((_, running)) => *running = running.plus(&amount),
                    None => class.sources.push((name, amount)),
                }
            }
        }
    }

    let mut sections = Vec::new();
    let mut classified = zero();
    for class in classes {
        let mut rows = class.sources;
        rows.sort_by(|one, two| {
            two.1
                .abs()
                .as_decimal()
                .cmp(one.1.abs().as_decimal())
                .then_with(|| one.0.cmp(&two.0))
        });
        let net = Money::sum(rows.iter().map(|(_, amount)| amount));
        classified = classified.plus(&net);
        sections.push(json!({
            "title": class.title,
            "lines": rows
                .iter()
                .map(|(name, amount)| json!({"description": name, "amount": wire(amount)}))
                .collect::<Vec<_>>(),
            "net": wire(&net),
            "quiet": class.quiet,
        }));
    }

    let cash_book_total = Money::sum(
        Records::new(graph, CASH_BANK_ACCOUNT)
            .list(scope, 1000)?
            .into_iter()
            .filter(|row| flag(row, "is_active"))
            .map(|row| money_or_nothing(&row, "current_balance"))
            .collect::<Result<Vec<_>, StoreError>>()?
            .iter(),
    );

    Ok(json!({
        "start_date": from,
        "end_date": until,
        "sections": sections,
        "net_change": wire(&movement),
        "opening_cash": wire(&opening),
        "closing_cash": wire(&opening.plus(&movement)),
        "unclassified": unclassified,
        // the classes should add up to the movement: when they do not, something moved cash without a counterpart
        "classified": wire(&classified),
        "cash_accounts": cash_accounts
            .iter()
            .map(|(code, name)| json!({"account_number": code, "name": name}))
            .collect::<Vec<_>>(),
        "cash_book_total": wire(&cash_book_total),
        "agrees_with_cash_book": cash_book_total == opening.plus(&movement),
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

    fn cash_book(graph: &vimbai_store::fake::MemoryGraph, code: &str, name: &str, balance: &str) {
        vimbai_store::Records::new(graph, CASH_BANK_ACCOUNT)
            .create(
                &scope(),
                &json!({"account_code": code, "account_name": name, "account_type": "bank",
                        "current_balance": balance, "opening_balance": balance, "is_active": true}),
            )
            .expect("a cash account");
    }

    fn chart(graph: &vimbai_store::fake::MemoryGraph) {
        for (number, name, kind, normal) in [
            ("1000", "Cash", "asset", "debit"),
            ("1500", "Equipment", "asset", "debit"),
            ("2000", "Loan", "liability", "credit"),
            ("3000", "Owner's stake", "equity", "credit"),
            ("4000", "Sales", "revenue", "credit"),
            ("5000", "Rent", "expense", "debit"),
        ] {
            vimbai_store::Records::new(graph, LEDGER_ACCOUNT)
                .create(
                    &scope(),
                    &json!({"account_number": number, "name": name, "account_type": kind,
                            "normal_balance": normal}),
                )
                .expect("an account");
        }
    }

    fn posted_entry(graph: &vimbai_store::fake::MemoryGraph, date: &str, lines: Value) {
        create_entry(
            graph,
            &scope(),
            &json!({"entry_date": date, "description": "moved", "status": "posted", "lines": lines}),
        )
        .expect("a posted entry");
    }

    fn section<'a>(answer: &'a Value, title: &str) -> &'a Value {
        answer["sections"]
            .as_array()
            .expect("sections")
            .iter()
            .find(|section| section["title"] == title)
            .expect("the section")
    }

    #[test]
    fn an_entry_dated_into_a_closed_month_is_refused_and_the_journal_is_left_alone() {
        use vimbai_store::fake::MemoryGraph;
        let graph = MemoryGraph::new();
        chart(&graph);
        posted_entry(
            &graph,
            "2026-10-20",
            json!([{"account_number": "1000", "debit": "500.00", "credit": "0"},
                   {"account_number": "4000", "debit": "0", "credit": "500.00"}]),
        );
        super::super::periods::close_period(
            &graph,
            &scope(),
            &json!({"period": "2026-10", "closes_on": "2026-11-05"}),
        )
        .expect("October closes");

        // the same trade again, dated inside the month that has been reported on
        let refused = create_entry(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-10-31", "description": "late invoice", "status": "posted",
                    "lines": [{"account_number": "1000", "debit": "90.00", "credit": "0"},
                              {"account_number": "4000", "debit": "0", "credit": "90.00"}]}),
        )
        .expect_err("October is closed");
        let words = refused.to_string();
        assert!(words.contains("2026-10 is closed"), "{words}");

        // nothing was written: the journal still holds the one entry it had
        assert_eq!(entries(&graph, &scope()).expect("journal").len(), 1);

        // a draft is a note rather than a posting, and posting it is refused for the same reason
        let note = create_entry(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-10-31", "description": "late invoice", "status": "draft",
                    "lines": [{"account_number": "1000", "debit": "90.00", "credit": "0"},
                              {"account_number": "4000", "debit": "0", "credit": "90.00"}]}),
        )
        .expect("a note is kept");
        let note_id = field(&note, "id").to_string();
        let refused = post_draft(&graph, &scope(), &note_id).expect_err("and cannot be posted");
        assert!(
            refused.to_string().contains("2026-10 is closed"),
            "{refused}"
        );

        // the same entry dated in the open month goes in
        posted_entry(
            &graph,
            "2026-11-02",
            json!([{"account_number": "1000", "debit": "90.00", "credit": "0"},
                   {"account_number": "4000", "debit": "0", "credit": "90.00"}]),
        );
        assert_eq!(entries(&graph, &scope()).expect("journal").len(), 2);
    }

    #[test]
    fn what_a_period_spent_is_signed_in_each_account_s_own_direction() {
        use vimbai_store::fake::MemoryGraph;
        let graph = MemoryGraph::new();
        chart(&graph);
        // a sale: cash up, sales up
        posted_entry(
            &graph,
            "2026-03-10",
            json!([{"account_number": "1000", "debit": "500.00", "credit": "0"},
                   {"account_number": "4000", "debit": "0", "credit": "500.00"}]),
        );
        // rent paid: expense up, cash down
        posted_entry(
            &graph,
            "2026-03-12",
            json!([{"account_number": "5000", "debit": "300.00", "credit": "0"},
                   {"account_number": "1000", "debit": "0", "credit": "300.00"}]),
        );

        let activity =
            posted_activity(&graph, &scope(), "2026-03-01", "2026-03-31").expect("activity");
        // spending is positive on an expense, earning is positive on revenue: neither reads as a minus
        assert_eq!(
            activity.get("5000").map(|m| m.to_fixed(2)),
            Some("300.00".to_string())
        );
        assert_eq!(
            activity.get("4000").map(|m| m.to_fixed(2)),
            Some("500.00".to_string())
        );
        // cash is debit-normal and net down 500 - 300
        assert_eq!(
            activity.get("1000").map(|m| m.to_fixed(2)),
            Some("200.00".to_string())
        );
    }

    #[test]
    fn a_draft_moves_nothing_and_a_date_outside_the_period_is_left_out() {
        use vimbai_store::fake::MemoryGraph;
        let graph = MemoryGraph::new();
        chart(&graph);
        create_entry(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-03-10", "description": "still a draft", "status": "draft",
                    "lines": [{"account_number": "5000", "debit": "900.00", "credit": "0"},
                              {"account_number": "1000", "debit": "0", "credit": "900.00"}]}),
        )
        .expect("a draft");
        posted_entry(
            &graph,
            "2026-04-02",
            json!([{"account_number": "5000", "debit": "50.00", "credit": "0"},
                   {"account_number": "1000", "debit": "0", "credit": "50.00"}]),
        );

        let activity =
            posted_activity(&graph, &scope(), "2026-03-01", "2026-03-31").expect("activity");
        // the draft is not spending, and April's rent is not March's
        assert!(activity.is_empty(), "{activity:?}");
    }

    #[test]
    fn a_sale_is_operating_cash_and_a_loan_is_financing() {
        use vimbai_store::fake::MemoryGraph;
        let graph = MemoryGraph::new();
        chart(&graph);
        cash_book(&graph, "1000", "Bank", "0");
        posted_entry(
            &graph,
            "2026-03-10",
            json!([{"account_number": "1000", "debit": "500.00", "credit": "0"},
                   {"account_number": "4000", "debit": "0", "credit": "500.00"}]),
        );
        posted_entry(
            &graph,
            "2026-03-11",
            json!([{"account_number": "1000", "debit": "2000.00", "credit": "0"},
                   {"account_number": "2000", "debit": "0", "credit": "2000.00"}]),
        );

        let answer = cash_flow(
            &graph,
            &scope(),
            &json!({"start_date": "2026-03-01", "end_date": "2026-03-31"}),
        )
        .expect("a statement");

        assert_eq!(answer["net_change"], "2500.000000");
        assert_eq!(answer["opening_cash"], "0.000000");
        assert_eq!(answer["closing_cash"], "2500.000000");
        assert_eq!(section(&answer, "Operating")["net"], "500.000000");
        assert_eq!(
            section(&answer, "Operating")["lines"][0]["description"],
            "Sales"
        );
        assert_eq!(section(&answer, "Financing")["net"], "2000.000000");
        assert_eq!(section(&answer, "Investing")["net"], "0.000000");
        // the classes account for every cent that moved
        assert_eq!(answer["classified"], answer["net_change"]);
    }

    #[test]
    fn money_out_reduces_the_class_it_came_from() {
        use vimbai_store::fake::MemoryGraph;
        let graph = MemoryGraph::new();
        chart(&graph);
        cash_book(&graph, "1000", "Bank", "0");
        posted_entry(
            &graph,
            "2026-03-12",
            json!([{"account_number": "5000", "debit": "300.00", "credit": "0"},
                   {"account_number": "1000", "debit": "0", "credit": "300.00"}]),
        );

        let answer = cash_flow(
            &graph,
            &scope(),
            &json!({"start_date": "2026-03-01", "end_date": "2026-03-31"}),
        )
        .expect("a statement");

        assert_eq!(answer["net_change"], "-300.000000");
        assert_eq!(section(&answer, "Operating")["net"], "-300.000000");
        assert_eq!(
            section(&answer, "Operating")["lines"][0]["amount"],
            "-300.000000"
        );
    }

    #[test]
    fn buying_equipment_is_investing_not_operating() {
        use vimbai_store::fake::MemoryGraph;
        let graph = MemoryGraph::new();
        chart(&graph);
        cash_book(&graph, "1000", "Bank", "0");
        posted_entry(
            &graph,
            "2026-03-13",
            json!([{"account_number": "1500", "debit": "800.00", "credit": "0"},
                   {"account_number": "1000", "debit": "0", "credit": "800.00"}]),
        );

        let answer = cash_flow(
            &graph,
            &scope(),
            &json!({"start_date": "2026-03-01", "end_date": "2026-03-31"}),
        )
        .expect("a statement");

        assert_eq!(section(&answer, "Investing")["net"], "-800.000000");
        assert_eq!(section(&answer, "Operating")["net"], "0.000000");
    }

    #[test]
    fn what_was_held_before_the_period_is_the_opening_balance() {
        use vimbai_store::fake::MemoryGraph;
        let graph = MemoryGraph::new();
        chart(&graph);
        cash_book(&graph, "1000", "Bank", "0");
        posted_entry(
            &graph,
            "2026-02-20",
            json!([{"account_number": "1000", "debit": "1000.00", "credit": "0"},
                   {"account_number": "3000", "debit": "0", "credit": "1000.00"}]),
        );
        posted_entry(
            &graph,
            "2026-03-05",
            json!([{"account_number": "1000", "debit": "250.00", "credit": "0"},
                   {"account_number": "4000", "debit": "0", "credit": "250.00"}]),
        );
        // later than the period: not this statement's business
        posted_entry(
            &graph,
            "2026-04-05",
            json!([{"account_number": "1000", "debit": "999.00", "credit": "0"},
                   {"account_number": "4000", "debit": "0", "credit": "999.00"}]),
        );

        let answer = cash_flow(
            &graph,
            &scope(),
            &json!({"start_date": "2026-03-01", "end_date": "2026-03-31"}),
        )
        .expect("a statement");

        assert_eq!(answer["opening_cash"], "1000.000000");
        assert_eq!(answer["net_change"], "250.000000");
        assert_eq!(answer["closing_cash"], "1250.000000");
    }

    #[test]
    fn a_draft_moves_no_cash() {
        use vimbai_store::fake::MemoryGraph;
        let graph = MemoryGraph::new();
        chart(&graph);
        cash_book(&graph, "1000", "Bank", "0");
        create_entry(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-03-15", "description": "sale", "status": "draft",
                    "lines": [{"account_number": "1000", "debit": "500.00", "credit": "0"},
                              {"account_number": "4000", "debit": "0", "credit": "500.00"}]}),
        )
        .expect("a draft");

        let answer = cash_flow(
            &graph,
            &scope(),
            &json!({"start_date": "2026-03-01", "end_date": "2026-03-31"}),
        )
        .expect("a statement");

        assert_eq!(answer["net_change"], "0.000000");
        assert_eq!(
            section(&answer, "Operating")["lines"]
                .as_array()
                .expect("lines")
                .len(),
            0
        );
    }

    #[test]
    fn a_book_with_no_cash_account_is_refused_rather_than_shown_as_zero() {
        use vimbai_store::fake::MemoryGraph;
        let graph = MemoryGraph::new();
        chart(&graph);

        let refused = cash_flow(
            &graph,
            &scope(),
            &json!({"start_date": "2026-03-01", "end_date": "2026-03-31"}),
        );
        assert!(
            refused.is_err(),
            "a Book with no cash account has no cash to report"
        );
    }

    #[test]
    fn the_cash_book_is_named_beside_the_statement_and_its_disagreement_is_visible() {
        use vimbai_store::fake::MemoryGraph;
        let graph = MemoryGraph::new();
        chart(&graph);
        // the cash book was opened with a balance that was never an entry
        cash_book(&graph, "1000", "Bank", "1200.00");
        posted_entry(
            &graph,
            "2026-03-05",
            json!([{"account_number": "1000", "debit": "250.00", "credit": "0"},
                   {"account_number": "4000", "debit": "0", "credit": "250.00"}]),
        );

        let answer = cash_flow(
            &graph,
            &scope(),
            &json!({"start_date": "2026-03-01", "end_date": "2026-03-31"}),
        )
        .expect("a statement");

        assert_eq!(answer["closing_cash"], "250.000000");
        assert_eq!(answer["cash_book_total"], "1200.000000");
        assert_eq!(answer["agrees_with_cash_book"], false);
        assert_eq!(answer["cash_accounts"][0]["name"], "Bank");
    }

    #[test]
    fn a_posted_entrys_lines_are_records_of_their_own() {
        use vimbai_store::fake::MemoryGraph;
        let graph = MemoryGraph::new();
        // a chart holding the two accounts the entry moves
        for (number, kind, normal) in [("1000", "asset", "debit"), ("4000", "revenue", "credit")] {
            vimbai_store::Records::new(&graph, LEDGER_ACCOUNT)
                .create(&scope(), &account(number, kind, normal))
                .expect("an account");
        }
        let posted = create_entry(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-03-15", "description": "sale", "status": "posted",
            "lines": [
                {"account_number": "1000", "debit": "250.00", "credit": "0"},
                {"account_number": "4000", "debit": "0", "credit": "250.00"}
            ]}),
        )
        .expect("posted");
        let id = field(&posted, "id").to_string();

        // the lines are their own records, each naming the entry and the side it moves
        let held = graph.held("book-1", "JournalLine");
        assert_eq!(held.len(), 2, "{held:?}");
        assert!(held.iter().all(|line| field(line, "entry_id") == id));
        let debited = held
            .iter()
            .find(|line| field(line, "account_number") == "1000")
            .expect("the asset line");
        assert_eq!(field(debited, "debit"), "250.000000");
        assert_eq!(field(debited, "credit"), "0.000000");
        assert_eq!(debited.get("line_no").and_then(Value::as_i64), Some(1));

        // the entry itself no longer carries the lines as a block: one shape, so the two can never disagree
        let stored = graph.held("book-1", "JournalEntry");
        assert_eq!(stored.len(), 1);
        assert!(
            stored[0].get("lines").is_none(),
            "the entry still carries its lines as a property: {}",
            stored[0]
        );

        // each line is joined to the account it moves, by the side it moves
        let debited_edges = graph.edges("DEBITED");
        let credited_edges = graph.edges("CREDITED");
        assert_eq!(debited_edges.len(), 1, "{debited_edges:?}");
        assert_eq!(credited_edges.len(), 1, "{credited_edges:?}");
        let asset = graph
            .held("book-1", "Account")
            .into_iter()
            .find(|account| field(account, "account_number") == "1000")
            .expect("the asset account");
        assert_eq!(debited_edges[0].to, field(&asset, "id"));
        assert_eq!(debited_edges[0].from, field(debited, "id"));
        // and every line still hangs off its entry
        assert_eq!(graph.edges("HAS_LINE").len(), 2);

        // and reading the entry hands the lines back, assembled from their own records
        let read = get_entry(&graph, &scope(), &id).expect("read back");
        let lines = read["lines"].as_array().expect("lines");
        assert_eq!(lines.len(), 2);
        assert_eq!(field(&lines[0], "account_number"), "1000");
        assert_eq!(field(&lines[1], "account_number"), "4000");
    }

    #[test]
    fn an_accounts_postings_are_found_through_the_ledger_edges() {
        use vimbai_store::fake::MemoryGraph;
        let graph = MemoryGraph::new();
        for (number, kind, normal) in [("1000", "asset", "debit"), ("4000", "revenue", "credit")] {
            vimbai_store::Records::new(&graph, LEDGER_ACCOUNT)
                .create(&scope(), &account(number, kind, normal))
                .expect("an account");
        }
        create_entry(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-03-15", "description": "sale", "status": "posted",
            "lines": [
                {"account_number": "1000", "debit": "250.00", "credit": "0"},
                {"account_number": "4000", "debit": "0", "credit": "250.00"}
            ]}),
        )
        .expect("posted");

        // the walk from the account finds the line that moved it, with the entry named beside it
        let moved = account_postings(&graph, &scope(), "1000", 100).expect("postings");
        let postings = moved["postings"].as_array().expect("postings");
        assert_eq!(postings.len(), 1, "{moved}");
        assert_eq!(field(&postings[0], "debit"), "250.000000");
        assert_eq!(moved["total_debit"], json!("250.000000"));

        // and the other side is a different account with its own posting
        let revenue = account_postings(&graph, &scope(), "4000", 100).expect("postings");
        assert_eq!(revenue["postings"].as_array().map(Vec::len), Some(1));
        assert_eq!(revenue["total_credit"], json!("250.000000"));

        // an account nothing moved answers with no postings rather than with everything
        let untouched = account_postings(&graph, &scope(), "9999", 100).expect("postings");
        assert_eq!(untouched["postings"].as_array().map(Vec::len), Some(0));
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
        assert_eq!(answer["account_type"], json!("asset"));
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
            // the chart, once per line, then the months closed, then the chain, then the write
            vec![
                account("1000", "asset", "debit"),
                account("4000", "revenue", "credit"),
            ],
            vec![
                account("1000", "asset", "debit"),
                account("4000", "revenue", "credit"),
            ],
            vec![],
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
                    "source_module": "Invoicing", "status": "posted",
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
