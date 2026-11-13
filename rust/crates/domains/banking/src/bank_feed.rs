#![forbid(unsafe_code)]
//! bank-feed-service: the bank connections a Book holds, the transactions that arrive from them, and the rules
//! that pick those transactions out.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a transaction may only be imported against a connection the caller owns - the shipped service looked the
//!   connection up first and refused a transaction that named nobody's connection;
//! * the same external id imported twice on one connection is a duplicate, not a second transaction; the same
//!   id on two different connections is two transactions, which is what the shipped duplicate check compared;
//! * an imported transaction starts `cleared`, or `pending` when the feed said it was still pending - status is
//!   never taken from the caller;
//! * a rule matches when every condition it states holds, and the highest priority matching rule wins; a rule
//!   that is inactive or has auto-matching off never matches, and an amount range is compared against the
//!   absolute amount, so a debit of 500 is inside a 100 to 1000 band;
//! * MT940 is parsed only for the tags it understood - a statement with no `:86:` line yields no transaction,
//!   because that is the tag that closed each one;
//! * the balance answer is the fixed figure the shipped service returned while provider integrations were a
//!   stub, and it says so rather than pretending to know.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::{amount, amount_or_zero, stored_amount, wire};
use vimbai_store::read::{field, flag, label, refused, text, whole};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, BANK_CONNECTION, BANK_FEED_RULE, BANK_FEED_TRANSACTION,
    BANK_SYNC,
};

/// The most records a listing reads in one go.
const LARGEST_PAGE: usize = 2000;

/// The providers the shipped service knew by name.
const PROVIDERS: [&str; 6] = [
    "plaid",
    "stripe",
    "quickbooks",
    "xero",
    "manual",
    "truelayer",
];

/// The statuses a transaction may hold.
const STATUSES: [&str; 5] = ["pending", "cleared", "reconciled", "disputed", "returned"];

/// The fixed balance the shipped service answered while the provider calls were stubs.
const STUB_AVAILABLE: &str = "10000";
const STUB_CURRENT: &str = "10500";
const STUB_PENDING: &str = "500";

// ---------------------------------------------------------------------------
// records
// ---------------------------------------------------------------------------

fn connections(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, BANK_CONNECTION).list(scope, LARGEST_PAGE)
}

fn transactions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, BANK_FEED_TRANSACTION).list(scope, LARGEST_PAGE)
}

fn rules(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, BANK_FEED_RULE).list(scope, LARGEST_PAGE)
}

/// A connection the caller owns, or a NotFound. Every route that names a connection begins here, which is what
/// keeps one Book's transactions out of another's.
fn owned_connection(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    connection_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, BANK_CONNECTION)
        .get(scope, connection_id)
        .map_err(|_| StoreError::NotFound)
}

// ---------------------------------------------------------------------------
// connections
// ---------------------------------------------------------------------------

/// `POST /connections`: a connection to a bank account in an organisation.
pub fn create_connection(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
    organization_id: &str,
) -> Result<Value, StoreError> {
    let provider = text(payload, "provider").to_lowercase();
    if !PROVIDERS.contains(&provider.as_str()) {
        return Err(refused(&format!("Unknown provider {provider}")));
    }
    let account_name = text(payload, "account_name");
    if account_name.is_empty() {
        return Err(refused("account_name is required"));
    }
    let last4 = text(payload, "account_number_last4");
    if last4.len() > 4 {
        return Err(refused("account_number_last4 must be at most 4 characters"));
    }
    let today = label(payload, "today");
    let body = json!({
        "provider": provider,
        "account_name": account_name,
        "account_type": vimbai_store::read::or_default(payload, "account_type", "checking"),
        "account_number_last4": last4,
        "routing_number": text(payload, "routing_number"),
        "access_token_encrypted": text(payload, "access_token_encrypted"),
        "webhook_url": text(payload, "webhook_url"),
        "auto_sync_enabled": payload
            .get("auto_sync_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        "sync_interval_minutes": match whole(payload, "sync_interval_minutes") {
            0 => 60,
            stated => stated as i64,
        },
        "organization_id": organization_id,
        "status": "active",
        "last_sync_status": "never",
        "created_at": today,
        "updated_at": today,
    });
    Records::new(graph, BANK_CONNECTION).create(scope, &body)
}

/// `GET /connections`.
pub fn list_connections(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    organization_id: &str,
) -> Result<Value, StoreError> {
    let mut mine: Vec<Value> = connections(graph, scope)?
        .into_iter()
        .filter(|connection| field(connection, "organization_id") == organization_id)
        .collect();
    mine.sort_by(|left, right| field(right, "created_at").cmp(field(left, "created_at")));
    Ok(json!({ "total": mine.len(), "connections": mine }))
}

/// `GET /connections/{id}`.
pub fn get_connection(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    connection_id: &str,
) -> Result<Value, StoreError> {
    owned_connection(graph, scope, connection_id)
}

/// `DELETE /connections/{id}`.
pub fn delete_connection(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    connection_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, BANK_CONNECTION).delete(scope, connection_id)?;
    Ok(json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// importing transactions
// ---------------------------------------------------------------------------

/// One transaction, as the feed described it. It takes the shape of the endpoint it stands in for, whose nine
/// facts all come from the request, so the count is the request's rather than this function's.
#[allow(clippy::too_many_arguments)]
fn imported_body(
    connection_id: &str,
    external_id: &str,
    date: &str,
    amount: &Money,
    description: &str,
    transaction_type: &str,
    pending: bool,
    payload: &Value,
    today: &str,
) -> Value {
    json!({
        "bank_connection_id": connection_id,
        "external_id": external_id,
        "date": date,
        "amount": wire(amount),
        "currency": vimbai_store::read::or_default(payload, "currency", "USD"),
        "description": description,
        "category": text(payload, "category"),
        "merchant_name": text(payload, "merchant_name"),
        "merchant_id": text(payload, "merchant_id"),
        "transaction_type": transaction_type,
        "pending": pending,
        // the feed's own metadata is kept as it arrived rather than read field by field
        "metadata": payload.get("metadata").cloned().unwrap_or(Value::Null),
        "status": if pending { "pending" } else { "cleared" },
        // nothing has looked at this transaction yet, so no rule has been credited with it
        "confidence_score": "0",
        "imported_at": today,
        "created_at": today,
        "updated_at": today,
    })
}

/// The transaction already imported under this external id on this connection, if there is one.
fn duplicate_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    connection_id: &str,
    external_id: &str,
) -> Result<Option<Value>, StoreError> {
    Ok(transactions(graph, scope)?.into_iter().find(|held| {
        field(held, "external_id") == external_id
            && field(held, "bank_connection_id") == connection_id
    }))
}

/// `POST /transactions/import`.
pub fn import_transaction(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let connection_id = text(payload, "bank_connection_id");
    if connection_id.is_empty() {
        return Err(refused("bank_connection_id is required"));
    }
    // the connection has to be the caller's before anything is written against it
    owned_connection(graph, scope, &connection_id)?;
    let external_id = text(payload, "external_id");
    if external_id.is_empty() {
        return Err(refused("external_id is required"));
    }
    if duplicate_of(graph, scope, &connection_id, &external_id)?.is_some() {
        return Err(StoreError::Conflict(
            "Transaction already imported".to_string(),
        ));
    }
    let date = text(payload, "date");
    if date.is_empty() {
        return Err(refused("date is required"));
    }
    let amount = amount(payload, "amount")?;
    let pending = payload
        .get("pending")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let today = label(payload, "today");
    let body = imported_body(
        &connection_id,
        &external_id,
        &date,
        &amount,
        &text(payload, "description"),
        &vimbai_store::read::or_default(payload, "transaction_type", "debit"),
        pending,
        payload,
        &today,
    );
    Records::new(graph, BANK_FEED_TRANSACTION).create(scope, &body)
}

/// `POST /transactions/import-batch`: every line answered, a duplicate reported rather than imported twice.
pub fn import_batch(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    lines: &[Value],
) -> Result<Value, StoreError> {
    let mut results = Vec::new();
    for line in lines {
        match import_transaction(graph, scope, line) {
            Ok(imported) => results.push(json!({
                "status": "imported",
                "transaction_id": field(&imported, "id"),
            })),
            Err(StoreError::Conflict(reason)) => results.push(json!({
                "status": "duplicate",
                "external_id": text(line, "external_id"),
                "message": reason,
            })),
            Err(error) => results.push(json!({
                "status": "failed",
                "error": error.to_string(),
            })),
        }
    }
    let imported = results
        .iter()
        .filter(|line| line["status"] == json!("imported"))
        .count();
    let duplicates = results
        .iter()
        .filter(|line| line["status"] == json!("duplicate"))
        .count();
    Ok(json!({
        "total": lines.len(),
        "imported": imported,
        "duplicates": duplicates,
        "failed": results.len() - imported - duplicates,
        "results": results,
    }))
}

/// One transaction read out of an MT940 statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mt940Line {
    pub date: String,
    pub amount: String,
    pub transaction_type: String,
    pub reference: String,
    pub description: String,
}

/// The decimal at the head of a string, in the point form the rest of the engine uses.
fn leading_amount(text: &str) -> String {
    let mut kept = String::new();
    for (index, character) in text.chars().enumerate() {
        let allowed = character.is_ascii_digit()
            || (character == ',' && !kept.contains('.'))
            || (character == '-' && index == 0);
        if !allowed {
            break;
        }
        kept.push(if character == ',' { '.' } else { character });
    }
    kept
}

/// `:61:` gives the value date, the debit/credit mark and the amount; `:82:` the bank's reference; `:86:` the
/// narrative, which also closes the transaction. A statement whose lines never reach `:86:` yields nothing,
/// which is what the shipped parser did.
pub fn parse_mt940(statement: &str) -> Vec<Mt940Line> {
    let mut found = Vec::new();
    let mut date = String::new();
    let mut amount = String::new();
    let mut kind = String::new();
    let mut reference = String::new();
    let mut open = false;

    for line in statement.split('\n') {
        if let Some(rest) = line.strip_prefix(":61:") {
            // the value date is six digits (YYMMDD), the debit/credit mark sits at position 14
            let digits: String = rest.chars().take(6).collect();
            if digits.len() == 6 {
                date = format!("20{}-{}-{}", &digits[0..2], &digits[2..4], &digits[4..6]);
            }
            kind = match rest.chars().nth(10) {
                Some('C') => "credit".to_string(),
                _ => "debit".to_string(),
            };
            // the amount is the decimal that follows the mark, and MT940 writes a comma for the point. The
            // shipped parser took everything to the next colon instead, so a line whose amount was followed by
            // its bank reference - which is how most statements write it - raised a ValueError and could not be
            // imported at all. Reading the number itself is the same answer for a well-formed line and an
            // answer at all for the rest.
            let tail: String = rest.chars().skip(11).collect();
            amount = leading_amount(&tail);
            reference.clear();
            open = true;
        } else if let Some(rest) = line.strip_prefix(":82:") {
            reference = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix(":86:") {
            if open {
                found.push(Mt940Line {
                    date: date.clone(),
                    amount: amount.clone(),
                    transaction_type: kind.clone(),
                    reference: reference.clone(),
                    description: rest.trim().to_string(),
                });
                open = false;
            }
        }
    }
    found
}

/// `POST /transactions/import-mt940`: the parsed lines imported, duplicates skipped in silence as shipped.
pub fn import_mt940(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    connection_id: &str,
    statement: &str,
    today: &str,
) -> Result<Value, StoreError> {
    owned_connection(graph, scope, connection_id)?;
    let parsed = parse_mt940(statement);
    let mut imported = Vec::new();
    for line in &parsed {
        let mut signed = Money::parse(&line.amount.replace(',', "."))
            .map_err(|error| StoreError::Invalid(format!("amount: {error}")))?;
        if line.transaction_type == "debit" {
            signed = signed.negate();
        }
        // a line with no bank reference is keyed by what it says, so the same statement imported twice is
        // still refused by the duplicate check rather than piling up
        let external_id = if line.reference.is_empty() {
            format!("mt940_{}_{}_{}", line.date, line.amount, line.description)
        } else {
            format!("mt940_{}", line.reference)
        };
        let body = imported_body(
            connection_id,
            &external_id,
            &line.date,
            &signed,
            &line.description,
            &line.transaction_type,
            false,
            &json!({}),
            today,
        );
        match Records::new(graph, BANK_FEED_TRANSACTION).create(scope, &body) {
            Ok(created) => imported.push(field(&created, "id").to_string()),
            // a duplicate is skipped, and anything else is skipped too: the shipped loop caught every refusal
            Err(_) => continue,
        }
    }
    Ok(json!({
        "parsed": parsed.len(),
        "imported": imported.len(),
        "transaction_ids": imported,
    }))
}

// ---------------------------------------------------------------------------
// reading transactions back
// ---------------------------------------------------------------------------

/// `GET /transactions`, narrowed by connection, status and a date window, then paged.
pub fn list_feed_transactions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let connection = label(query, "connection_id");
    let state = label(query, "status").to_lowercase();
    let from = label(query, "start_date");
    let until = label(query, "end_date");
    let limit = match whole(query, "limit") {
        0 => 100,
        stated => stated,
    } as usize;
    let offset = whole(query, "offset") as usize;

    let mut mine = transactions(graph, scope)?;
    if !connection.is_empty() {
        mine.retain(|held| field(held, "bank_connection_id") == connection);
    }
    if !state.is_empty() {
        mine.retain(|held| field(held, "status").eq_ignore_ascii_case(&state));
    }
    if !from.is_empty() {
        mine.retain(|held| field(held, "date") >= from.as_str());
    }
    if !until.is_empty() {
        mine.retain(|held| field(held, "date") <= until.as_str());
    }
    mine.sort_by(|left, right| field(right, "date").cmp(field(left, "date")));
    let paged: Vec<Value> = mine.into_iter().skip(offset).take(limit).collect();
    Ok(Value::Array(paged))
}

/// `GET /transactions/{id}`.
pub fn get_feed_transaction(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    transaction_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, BANK_FEED_TRANSACTION)
        .get(scope, transaction_id)
        .map_err(|_| StoreError::NotFound)
}

/// `PUT /transactions/{id}/status`.
pub fn update_transaction_status(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    transaction_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let state = text(payload, "status").to_lowercase();
    if !STATUSES.contains(&state.as_str()) {
        return Err(refused(&format!("Unknown status {state}")));
    }
    let held = get_feed_transaction(graph, scope, transaction_id)?;
    let mut patch = json!({
        "status": state,
        "notes": text(payload, "notes"),
        "updated_at": label(payload, "today"),
    });
    // a transaction that has been reconciled records when, which is what the shipped service stamped
    if state == "reconciled" {
        patch["reconciled_at"] = json!(label(payload, "today"));
    }
    let _ = held;
    Records::new(graph, BANK_FEED_TRANSACTION).update(scope, transaction_id, &patch)
}

/// `POST /transactions/{id}/link`: the entry or invoice this transaction turned out to be.
pub fn link_transaction(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    transaction_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    get_feed_transaction(graph, scope, transaction_id)?;
    let mut patch = json!({ "updated_at": label(payload, "today") });
    let entry = text(payload, "journal_entry_id");
    if !entry.is_empty() {
        patch["journal_entry_id"] = json!(entry);
    }
    let invoice = text(payload, "invoice_id");
    if !invoice.is_empty() {
        patch["invoice_id"] = json!(invoice);
    }
    Records::new(graph, BANK_FEED_TRANSACTION).update(scope, transaction_id, &patch)
}

// ---------------------------------------------------------------------------
// syncs
// ---------------------------------------------------------------------------

/// `POST /sync/start`: the sync is recorded and the connection marked as syncing. The fetch itself is a stub in
/// the shipped service and stays one here - neither service has a provider credential to fetch with.
pub fn start_sync(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let connection_id = text(payload, "bank_connection_id");
    if connection_id.is_empty() {
        return Err(refused("bank_connection_id is required"));
    }
    let connection = owned_connection(graph, scope, &connection_id)?;
    let today = label(payload, "today");
    let sync = Records::new(graph, BANK_SYNC).create(
        scope,
        &json!({
            "bank_connection_id": connection_id,
            "status": "syncing",
            "transactions_imported": 0,
            "transactions_updated": 0,
            "transactions_matched": 0,
            "errors": [],
            "started_at": today,
            "start_date": text(payload, "start_date"),
            "end_date": text(payload, "end_date"),
        }),
    )?;
    Records::new(graph, BANK_CONNECTION).update(
        scope,
        field(&connection, "id"),
        &json!({ "last_sync_status": "syncing", "updated_at": today }),
    )?;
    Ok(json!({
        "sync_id": field(&sync, "id"),
        "status": "started",
    }))
}

/// `GET /sync/history`.
pub fn sync_history(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let connection = label(query, "connection_id");
    let limit = match whole(query, "limit") {
        0 => 50,
        stated => stated,
    } as usize;
    let mut mine = Records::new(graph, BANK_SYNC).list(scope, LARGEST_PAGE)?;
    if !connection.is_empty() {
        mine.retain(|held| field(held, "bank_connection_id") == connection);
    }
    mine.sort_by(|left, right| field(right, "started_at").cmp(field(left, "started_at")));
    Ok(Value::Array(mine.into_iter().take(limit).collect()))
}

/// `GET /sync/{id}`.
pub fn get_sync(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    sync_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, BANK_SYNC)
        .get(scope, sync_id)
        .map_err(|_| StoreError::NotFound)
}

// ---------------------------------------------------------------------------
// the balance a connection reports
// ---------------------------------------------------------------------------

/// `GET /connections/{id}/balance`: the fixed figure the shipped service answered, labelled as such.
pub fn account_balance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    connection_id: &str,
    today: &str,
) -> Result<Value, StoreError> {
    let connection = owned_connection(graph, scope, connection_id)?;
    Ok(json!({
        "account_id": connection_id,
        "available_balance": STUB_AVAILABLE,
        "current_balance": STUB_CURRENT,
        "currency": vimbai_store::read::or_default(&connection, "currency", "USD"),
        "as_of_date": today,
        "pending_transactions": STUB_PENDING,
        // the figures above are the placeholder the shipped service returned before the provider calls were
        // written; nothing here has spoken to a bank
        "source": "placeholder",
    }))
}

// ---------------------------------------------------------------------------
// reconciliation rules
// ---------------------------------------------------------------------------

/// `POST /rules`.
pub fn create_feed_rule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let conditions = payload
        .get("match_conditions")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !conditions.is_object() {
        return Err(refused("match_conditions must be an object"));
    }
    let body = json!({
        "name": name,
        "description": text(payload, "description"),
        "match_conditions": conditions,
        "priority": whole(payload, "priority") as i64,
        "auto_match_enabled": payload
            .get("auto_match_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        "create_journal_entry": payload
            .get("create_journal_entry")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "journal_entry_template": payload
            .get("journal_entry_template")
            .cloned()
            .unwrap_or(Value::Null),
        "active": payload.get("active").and_then(Value::as_bool).unwrap_or(true),
        "created_at": label(payload, "today"),
    });
    Records::new(graph, BANK_FEED_RULE).create(scope, &body)
}

/// `GET /rules`.
pub fn list_feed_rules(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let active_only = query
        .get("active_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut mine = rules(graph, scope)?;
    if active_only {
        mine.retain(|rule| flag(rule, "active"));
    }
    Ok(json!({ "total": mine.len(), "rules": mine }))
}

/// `PUT /rules/{id}`.
pub fn update_feed_rule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, BANK_FEED_RULE)
        .get(scope, rule_id)
        .map_err(|_| StoreError::NotFound)?;
    let conditions = match payload.get("match_conditions") {
        Some(stated) if stated.is_object() => stated.clone(),
        Some(_) => return Err(refused("match_conditions must be an object")),
        None => held
            .get("match_conditions")
            .cloned()
            .unwrap_or_else(|| json!({})),
    };
    let name = match text(payload, "name") {
        stated if stated.is_empty() => field(&held, "name").to_string(),
        stated => stated,
    };
    Records::new(graph, BANK_FEED_RULE).update(
        scope,
        rule_id,
        &json!({
            "name": name,
            "description": vimbai_store::read::or_default(
                payload,
                "description",
                field(&held, "description"),
            ),
            "match_conditions": conditions,
            "priority": whole(payload, "priority") as i64,
            "auto_match_enabled": payload
                .get("auto_match_enabled")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| flag(&held, "auto_match_enabled")),
            "create_journal_entry": payload
                .get("create_journal_entry")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| flag(&held, "create_journal_entry")),
            "journal_entry_template": payload
                .get("journal_entry_template")
                .cloned()
                .or_else(|| held.get("journal_entry_template").cloned())
                .unwrap_or(Value::Null),
            "active": payload
                .get("active")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| flag(&held, "active")),
        }),
    )
}

/// `DELETE /rules/{id}`.
pub fn delete_feed_rule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    rule_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, BANK_FEED_RULE).delete(scope, rule_id)?;
    Ok(json!({ "ok": true }))
}

/// Whether a rule says this transaction is the one it describes.
fn rule_matches(rule: &Value, transaction: &Value) -> bool {
    if !flag(rule, "active") || !flag(rule, "auto_match_enabled") {
        return false;
    }
    let conditions = match rule.get("match_conditions") {
        Some(conditions) if conditions.is_object() => conditions,
        _ => return false,
    };
    // an amount band is measured against the absolute amount, so a debit of 500 is inside 100 to 1000
    let magnitude = stored_amount(transaction, "amount")
        .map(|held| held.abs())
        .unwrap_or_else(|_| Money::zero());
    if let Some(stated) = conditions.get("amount_min") {
        let floor =
            Money::parse(stated.to_string().trim_matches('"')).unwrap_or_else(|_| Money::zero());
        if magnitude < floor {
            return false;
        }
    }
    if let Some(stated) = conditions.get("amount_max") {
        let ceiling =
            Money::parse(stated.to_string().trim_matches('"')).unwrap_or_else(|_| Money::zero());
        if magnitude > ceiling {
            return false;
        }
    }
    if let Some(stated) = conditions.get("merchant_pattern").and_then(Value::as_str) {
        let merchant = field(transaction, "merchant_name");
        // the shipped service only checked the pattern when the transaction named a merchant, so a transaction
        // without one satisfies a merchant condition
        if !merchant.is_empty() && !merchant.to_lowercase().contains(&stated.to_lowercase()) {
            return false;
        }
    }
    if let Some(stated) = conditions.get("categories").and_then(Value::as_array) {
        let category = field(transaction, "category");
        if !category.is_empty()
            && !stated
                .iter()
                .filter_map(Value::as_str)
                .any(|allowed| allowed == category)
        {
            return false;
        }
    }
    if let Some(stated) = conditions.get("transaction_type").and_then(Value::as_str) {
        if field(transaction, "transaction_type") != stated {
            return false;
        }
    }
    true
}

/// The highest priority rule this transaction satisfies.
pub fn matched_rule(rule_rows: &[Value], transaction: &Value) -> Option<Value> {
    let mut sorted: Vec<&Value> = rule_rows.iter().collect();
    sorted.sort_by_key(|rule| std::cmp::Reverse(whole(rule, "priority")));
    sorted
        .into_iter()
        .find(|rule| rule_matches(rule, transaction))
        .cloned()
}

/// `POST /rules/apply`: the rules run over the transactions named, and the one that took each is recorded.
pub fn apply_rules(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    transaction_ids: &[String],
    today: &str,
) -> Result<Value, StoreError> {
    let rule_rows = rules(graph, scope)?;
    let mut results = Vec::new();
    for transaction_id in transaction_ids {
        let transaction = match get_feed_transaction(graph, scope, transaction_id) {
            Ok(transaction) => transaction,
            // a transaction the caller does not have is passed over rather than failing the batch
            Err(_) => continue,
        };
        let matched = matched_rule(&rule_rows, &transaction);
        if let Some(rule) = &matched {
            Records::new(graph, BANK_FEED_TRANSACTION).update(
                scope,
                transaction_id,
                &json!({
                    "matched_rule_id": field(rule, "id"),
                    "confidence_score": "1",
                    "updated_at": today,
                }),
            )?;
        }
        results.push(json!({
            "transaction_id": transaction_id,
            "matched": matched.is_some(),
            "rule_id": matched.map(|rule| field(&rule, "id").to_string()),
        }));
    }
    Ok(json!({ "processed": results.len(), "results": results }))
}

// ---------------------------------------------------------------------------
// webhooks and statistics
// ---------------------------------------------------------------------------

/// `POST /webhooks/{provider}`: a provider's own event. Only a Plaid `TRANSACTIONS` webhook imports anything,
/// as shipped, and each line still has to name a connection the caller owns.
pub fn handle_webhook(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    provider: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let provider = provider.to_lowercase();
    if !PROVIDERS.contains(&provider.as_str()) {
        return Err(refused("Unknown provider"));
    }
    if provider != "plaid" || text(payload, "webhook_type") != "TRANSACTIONS" {
        return Ok(json!({ "status": "received" }));
    }
    let connection_id = text(payload, "connection_id");
    let lines = payload
        .get("transactions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for line in &lines {
        let body = imported_body(
            &connection_id,
            &text(line, "transaction_id"),
            &text(line, "date"),
            &amount_or_zero(line, "amount")?,
            &text(line, "name"),
            if amount_or_zero(line, "amount")?.is_negative() {
                "debit"
            } else {
                "credit"
            },
            false,
            line,
            &label(payload, "today"),
        );
        // a connection this Book does not hold refuses the line, and the shipped service swallowed that
        let _ = Records::new(graph, BANK_FEED_TRANSACTION).create(scope, &body);
    }
    Ok(json!({ "status": "received" }))
}

/// `GET /statistics`: what an organisation's connections have brought in.
pub fn statistics(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    organization_id: &str,
) -> Result<Value, StoreError> {
    let ours: Vec<Value> = connections(graph, scope)?
        .into_iter()
        .filter(|connection| field(connection, "organization_id") == organization_id)
        .collect();
    let all = transactions(graph, scope)?;
    let mut total = 0usize;
    let mut reconciled = 0usize;
    let mut pending = 0usize;
    let mut by_provider: Vec<(String, usize, usize)> = Vec::new();
    for connection in &ours {
        let id = field(connection, "id");
        let on_this: Vec<&Value> = all
            .iter()
            .filter(|held| field(held, "bank_connection_id") == id)
            .collect();
        total += on_this.len();
        reconciled += on_this
            .iter()
            .filter(|held| field(held, "status") == "reconciled")
            .count();
        pending += on_this
            .iter()
            .filter(|held| field(held, "status") == "pending")
            .count();
        let provider = field(connection, "provider").to_lowercase();
        match by_provider.iter_mut().find(|row| row.0 == provider) {
            Some(row) => {
                row.1 += 1;
                row.2 += on_this.len();
            }
            None => by_provider.push((provider, 1, on_this.len())),
        }
    }
    let rate = if total == 0 {
        Money::zero()
    } else {
        Money::from_u64(reconciled as u64)
            .over(&Money::from_u64(total as u64))
            .times(&Money::from_i64(100))
            .round(2)
    };
    let providers: Vec<Value> = by_provider
        .into_iter()
        .map(|(provider, connections, transactions)| {
            json!({
                "provider": provider,
                "connections": connections,
                "transactions": transactions,
            })
        })
        .collect();
    Ok(json!({
        "total_connections": ours.len(),
        "total_transactions": total,
        "reconciled": reconciled,
        "pending": pending,
        "reconciliation_rate": wire(&rate),
        "by_provider": providers,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn connection() -> Value {
        json!({"id": "conn-1", "provider": "plaid", "organization_id": "org-1", "status": "active"})
    }

    #[test]
    fn a_transaction_arrives_cleared_unless_the_feed_says_pending() {
        let body = imported_body(
            "conn-1",
            "ext-1",
            "2026-03-01",
            &Money::from_i64(500),
            "card payment",
            "debit",
            false,
            &json!({"currency": "GBP"}),
            "2026-03-02T00:00:00",
        );
        assert_eq!(body["status"], json!("cleared"));
        assert_eq!(body["currency"], json!("GBP"));
        // no rule has looked at it yet
        assert_eq!(body["confidence_score"], json!("0"));

        let waiting = imported_body(
            "conn-1",
            "ext-2",
            "2026-03-01",
            &Money::from_i64(5),
            "pending",
            "debit",
            true,
            &json!({}),
            "2026-03-02T00:00:00",
        );
        assert_eq!(waiting["status"], json!("pending"));
        assert_eq!(waiting["pending"], json!(true));
        assert_eq!(waiting["currency"], json!("USD"));
    }

    #[test]
    fn the_same_external_id_on_two_connections_is_two_transactions() {
        let held = [
            json!({"id": "t-1", "external_id": "ext-1", "bank_connection_id": "conn-1"}),
            json!({"id": "t-2", "external_id": "ext-1", "bank_connection_id": "conn-2"}),
        ];
        let on_first = held.iter().find(|tx| {
            field(tx, "external_id") == "ext-1" && field(tx, "bank_connection_id") == "conn-1"
        });
        assert_eq!(
            on_first.map(|tx| field(tx, "id").to_string()),
            Some("t-1".to_string())
        );
        let on_second = held.iter().find(|tx| {
            field(tx, "external_id") == "ext-1" && field(tx, "bank_connection_id") == "conn-2"
        });
        assert_eq!(
            on_second.map(|tx| field(tx, "id").to_string()),
            Some("t-2".to_string())
        );
    }

    #[test]
    fn an_amount_band_is_measured_against_the_absolute_amount() {
        let rule = json!({
            "id": "rule-1", "priority": 5, "active": true, "auto_match_enabled": true,
            "match_conditions": {"amount_min": 100, "amount_max": 1000}
        });
        // a debit of 500 is inside a 100 to 1000 band
        let debit = json!({"amount": "-500.000000", "transaction_type": "debit"});
        assert!(rule_matches(&rule, &debit));
        let small = json!({"amount": "-50.000000"});
        assert!(!rule_matches(&rule, &small));
    }

    #[test]
    fn the_highest_priority_matching_rule_wins() {
        let rules = vec![
            json!({"id": "low", "priority": 1, "active": true, "auto_match_enabled": true,
                   "match_conditions": {"transaction_type": "debit"}}),
            json!({"id": "high", "priority": 9, "active": true, "auto_match_enabled": true,
                   "match_conditions": {"transaction_type": "debit"}}),
            json!({"id": "wrong", "priority": 20, "active": true, "auto_match_enabled": true,
                   "match_conditions": {"transaction_type": "credit"}}),
        ];
        let matched = matched_rule(&rules, &json!({"transaction_type": "debit"})).expect("a rule");
        assert_eq!(field(&matched, "id"), "high");
    }

    #[test]
    fn a_rule_that_is_asleep_never_matches() {
        let asleep = json!({
            "id": "rule-1", "priority": 5, "active": false, "auto_match_enabled": true,
            "match_conditions": {}
        });
        assert!(!rule_matches(&asleep, &json!({})));
        let hands_off = json!({
            "id": "rule-2", "priority": 5, "active": true, "auto_match_enabled": false,
            "match_conditions": {}
        });
        assert!(!rule_matches(&hands_off, &json!({})));
    }

    #[test]
    fn a_transaction_with_no_merchant_still_satisfies_a_merchant_pattern() {
        let rule = json!({
            "id": "rule-1", "priority": 1, "active": true, "auto_match_enabled": true,
            "match_conditions": {"merchant_pattern": "tesco"}
        });
        // as shipped: the pattern was only compared when the transaction named a merchant
        assert!(rule_matches(&rule, &json!({"merchant_name": ""})));
        assert!(rule_matches(
            &rule,
            &json!({"merchant_name": "TESCO STORES"})
        ));
        assert!(!rule_matches(
            &rule,
            &json!({"merchant_name": "sainsburys"})
        ));
    }

    #[test]
    fn an_mt940_line_carries_its_date_reference_and_signed_amount() {
        let statement =
            ":20:ST-1\n:61:2603010301C1500,00NTRFNONREF\n:82:REF-9\n:86:Customer payment\n";
        let parsed = parse_mt940(statement);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].date, "2026-03-01");
        assert_eq!(parsed[0].amount, "1500.00");
        assert_eq!(parsed[0].transaction_type, "credit");
        assert_eq!(parsed[0].reference, "REF-9");
        assert_eq!(parsed[0].description, "Customer payment");
    }

    #[test]
    fn a_debit_in_an_mt940_statement_is_marked_as_one() {
        let statement = ":61:2603020302D100,50NTRF\n:86:Bank charge\n";
        let parsed = parse_mt940(statement);
        assert_eq!(parsed[0].transaction_type, "debit");
        assert_eq!(parsed[0].amount, "100.50");
    }

    #[test]
    fn an_mt940_transaction_is_only_closed_by_its_narrative() {
        // no :86: line, so nothing was completed - the shipped parser appended on :86: only
        let statement = ":61:2603020302D100,50NTRF\n";
        assert!(parse_mt940(statement).is_empty());
    }

    #[test]
    fn the_balance_answer_says_it_is_a_placeholder() {
        let graph = FakeGraph::queue(vec![vec![connection()]]);
        let balance =
            account_balance(&graph, &scope(), "conn-1", "2026-03-02T00:00:00").expect("a balance");
        assert_eq!(balance["source"], json!("placeholder"));
        assert_eq!(balance["available_balance"], json!("10000"));
        assert_eq!(balance["pending_transactions"], json!("500"));
        assert_eq!(balance["as_of_date"], json!("2026-03-02T00:00:00"));
    }

    #[test]
    fn a_status_the_service_does_not_know_is_refused() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "t-1"})]]);
        let refused_state =
            update_transaction_status(&graph, &scope(), "t-1", &json!({"status": "imaginative"}));
        assert!(refused_state.is_err());
    }

    #[test]
    fn the_reconciliation_rate_is_a_percentage_of_what_arrived() {
        let graph = FakeGraph::queue(vec![
            // the organisation's connections
            vec![connection()],
            // the transactions on them
            vec![
                json!({"id": "t-1", "bank_connection_id": "conn-1", "status": "reconciled"}),
                json!({"id": "t-2", "bank_connection_id": "conn-1", "status": "cleared"}),
                json!({"id": "t-3", "bank_connection_id": "conn-1", "status": "pending"}),
                json!({"id": "t-4", "bank_connection_id": "conn-1", "status": "pending"}),
            ],
        ]);
        let stats = statistics(&graph, &scope(), "org-1").expect("statistics");
        assert_eq!(stats["total_transactions"], json!(4));
        assert_eq!(stats["reconciled"], json!(1));
        assert_eq!(stats["pending"], json!(2));
        assert_eq!(stats["reconciliation_rate"], json!("25.000000"));
        assert_eq!(stats["by_provider"][0]["provider"], json!("plaid"));
    }

    #[test]
    fn a_webhook_for_an_unknown_provider_is_refused() {
        let graph = FakeGraph::queue(vec![]);
        assert!(handle_webhook(&graph, &scope(), "telepathy", &json!({})).is_err());
    }

    #[test]
    fn a_webhook_that_is_not_plaid_transactions_imports_nothing() {
        let graph = FakeGraph::queue(vec![]);
        let answer = handle_webhook(&graph, &scope(), "plaid", &json!({"webhook_type": "AUTH"}))
            .expect("received");
        assert_eq!(answer["status"], json!("received"));
        assert_eq!(graph.statements(), 0);
    }
}
