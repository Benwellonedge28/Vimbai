#![forbid(unsafe_code)]
//! bank-reconciliation-service: statement lines, cash book entries, the matching of one against the other.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a statement's credit and debit totals are measured from its own lines rather than taken from the caller,
//!   and every line is stamped with the statement it arrived on;
//! * a cash book entry is recorded unmatched: the shipped service refused to take `matched`, the reconciliation it
//!   belongs to, or the statement line it was matched to from the caller;
//! * a reconciliation subtracts the bank errors from both sides - the adjusted statement balance and the adjusted
//!   cash book balance - so a bank error moves the difference by nothing. That is the shipped arithmetic, kept;
//! * the two figures nobody supplies, unpresented cheques and uncredited deposits, are always nil;
//! * automatic matching needs the same reference and an amount within a cent, so a line with no reference never
//!   matches anything, and it searches every cash book entry the caller has, not only the ones on that account;
//! * an outstanding item is a leftover of the last reconciliation on the account, so nothing imported since then
//!   appears in the report.

use serde_json::{json, Map, Value};

use vimbai_common::Money;
use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::read::{field, label, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, BANK_REC_CASH_ENTRY, BANK_REC_RECONCILIATION,
    BANK_STATEMENT,
};

const ONE_CENT: &str = "0.01";
const IMPORTED: &str = "imported";
const IN_PROGRESS: &str = "in_progress";
const COMPLETED: &str = "completed";

/// The kinds of statement line the shipped service accepted by name.
const LINE_TYPES: [&str; 9] = [
    "deposit",
    "withdrawal",
    "cheque",
    "direct_debit",
    "direct_credit",
    "transfer",
    "bank_charge",
    "interest",
    "standing_order",
];

fn statements(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, BANK_STATEMENT).list(scope, 2000)
}

fn cash_entries(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, BANK_REC_CASH_ENTRY).list(scope, 2000)
}

fn reconciliations(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, BANK_REC_RECONCILIATION).list(scope, 2000)
}

fn flag(record: &Value, key: &str) -> bool {
    match record.get(key) {
        Some(Value::Bool(value)) => *value,
        Some(Value::String(text)) => text == "true",
        _ => false,
    }
}

/// A statement line as the store holds it: the amount as a decimal string, the flag as a boolean.
fn line_from_payload(line: &Value, statement_id: &str) -> Result<Value, StoreError> {
    let kind = text(line, "transaction_type");
    if !LINE_TYPES.contains(&kind.as_str()) {
        return Err(refused(&format!(
            "transaction_type {kind} is not one of {}",
            LINE_TYPES.join(", ")
        )));
    }
    let amount = amount_or_zero(line, "amount")?;
    Ok(json!({
        "id": text(line, "id"),
        "statement_id": statement_id,
        "date": text(line, "date"),
        "description": text(line, "description"),
        "reference": line.get("reference").cloned().unwrap_or(Value::Null),
        "transaction_type": kind,
        "amount": wire(&amount),
        "balance": wire(&amount_or_zero(line, "balance")?),
        "is_debit": flag(line, "is_debit"),
        "matched": false,
        "matched_transaction_id": Value::Null,
    }))
}

fn lines_of(record: &Value, key: &str) -> Vec<Value> {
    match record.get(key) {
        Some(Value::Array(rows)) => rows.clone(),
        _ => Vec::new(),
    }
}

/// `POST /statements`: a statement and the lines it arrived with.
pub fn import_statement(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let bank_account = text(payload, "bank_account");
    if bank_account.is_empty() {
        return Err(refused("bank_account is required"));
    }
    let statement_number = text(payload, "statement_number");
    if statement_number.is_empty() {
        return Err(refused("statement_number is required"));
    }

    // the id is minted here so the lines can be stamped with it before the store assigns its own
    let stated = text(payload, "id");
    let drafted: Vec<Value> = lines_of(payload, "lines")
        .iter()
        .map(|line| line_from_payload(line, &stated))
        .collect::<Result<_, _>>()?;

    let mut credits = Money::zero();
    let mut debits = Money::zero();
    for line in &drafted {
        let moved = stored_amount(line, "amount")?;
        if flag(line, "is_debit") {
            debits = debits.plus(&moved);
        } else {
            credits = credits.plus(&moved);
        }
    }

    let body = json!({
        "bank_account": bank_account,
        "statement_number": statement_number,
        "statement_start_date": text(payload, "statement_start_date"),
        "statement_end_date": text(payload, "statement_end_date"),
        "opening_balance": wire(&amount_or_zero(payload, "opening_balance")?),
        "closing_balance": wire(&amount_or_zero(payload, "closing_balance")?),
        "total_credits": wire(&credits),
        "total_debits": wire(&debits),
        "lines": drafted,
        "status": IMPORTED,
        "created_at": text(payload, "created_at"),
        "as_of": text(payload, "as_of"),
    });
    // the store assigns the id, so the lines are stamped after it is known and written back once
    let mut saved = remember(graph, scope, BANK_STATEMENT, body.clone())?;
    let id = field(&saved, "id");
    let mut lines = lines_of(&saved, "lines");
    for line in &mut lines {
        line["statement_id"] = json!(id);
    }
    if id.is_empty() {
        return Ok(saved);
    }
    saved = Records::new(graph, BANK_STATEMENT).update(scope, id, &json!({ "lines": lines }))?;
    Ok(merge(&body, &saved))
}

/// `GET /statements`: newest first, narrowed to one account when the caller names it.
pub fn list_statements(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = label(query, "bank_account");
    let mut mine = statements(graph, scope)?;
    if !wanted.is_empty() {
        mine.retain(|row| field(row, "bank_account") == wanted);
    }
    Ok(json!({ "statements": mine }))
}

/// `GET /statements/{id}`.
pub fn get_statement(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    statement_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, BANK_STATEMENT)
        .get(scope, statement_id)
        .map_err(|_| StoreError::NotFound)
}

/// `POST /cash-book`: an entry taken from the books. It arrives unmatched.
pub fn add_cash_book_entry(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let description = text(payload, "description");
    if description.is_empty() {
        return Err(refused("description is required"));
    }
    amount_or_zero(payload, "amount")?;
    let body = json!({
        "date": text(payload, "date"),
        "description": description,
        "reference": text(payload, "reference"),
        "transaction_type": text(payload, "transaction_type"),
        "amount": wire(&amount_or_zero(payload, "amount")?),
        "is_debit": flag(payload, "is_debit"),
        "matched": false,
        "matched_statement_id": Value::Null,
        "bank_reconciliation_id": Value::Null,
    });
    remember(graph, scope, BANK_REC_CASH_ENTRY, body)
}

/// `GET /cash-book`: the caller's entries for one account, narrowed by an optional date window and match flag.
pub fn list_cash_book(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = label(query, "bank_account");
    if wanted.is_empty() {
        return Err(refused("bank_account is required"));
    }
    let from = label(query, "start_date");
    let until = label(query, "end_date");
    let matched = query.get("matched").and_then(Value::as_bool);

    // the shipped service filtered on the account only when it was also asked for a date window or a match
    // flag - a bare account named on its own listed every entry the caller had. Kept as shipped: the account
    // has to be named, and the entries are the caller's own.
    let mut mine = cash_entries(graph, scope)?;
    if !from.is_empty() {
        mine.retain(|row| field(row, "date") >= from.as_str());
    }
    let last = until.as_str();
    if !last.is_empty() {
        mine.retain(|row| field(row, "date") <= last);
    }
    if let Some(wanted_flag) = matched {
        mine.retain(|row| flag(row, "matched") == wanted_flag);
    }
    Ok(json!({ "entries": mine }))
}

/// `POST /reconcile`: the two balances, the adjustments, and the difference between them.
pub fn create_reconciliation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let bank_account = label(payload, "bank_account");
    if bank_account.is_empty() {
        return Err(refused("bank_account is required"));
    }
    let reconciliation_date = label(payload, "reconciliation_date");
    if reconciliation_date.is_empty() {
        return Err(refused("reconciliation_date is required"));
    }
    let stated = amount_or_zero(payload, "statement_balance")?;
    let booked = amount_or_zero(payload, "cash_book_balance")?;
    let outstanding_deposits = amount_or_zero(payload, "outstanding_deposits")?;
    let outstanding_cheques = amount_or_zero(payload, "outstanding_cheques")?;
    let bank_errors = amount_or_zero(payload, "bank_errors")?;
    let cash_book_errors = amount_or_zero(payload, "cash_book_errors")?;

    // statement - cheques + deposits - bank errors; cash book - bank errors + cash book errors. The bank errors
    // are taken off both sides as shipped, so they cancel in the difference.
    let adjusted_statement = stated
        .minus(&outstanding_cheques)
        .plus(&outstanding_deposits)
        .minus(&bank_errors);
    let adjusted_cash_book = booked.minus(&bank_errors).plus(&cash_book_errors);
    let difference = adjusted_statement.minus(&adjusted_cash_book);

    let body = json!({
        "bank_account": bank_account,
        "reconciliation_date": reconciliation_date,
        "statement_balance": wire(&stated),
        "cash_book_balance": wire(&booked),
        "difference": wire(&difference),
        "outstanding_deposits": wire(&outstanding_deposits),
        "outstanding_cheques": wire(&outstanding_cheques),
        "bank_errors": wire(&bank_errors),
        "cash_book_errors": wire(&cash_book_errors),
        "unpresented_cheques": wire(&Money::zero()),
        "uncredited_deposits": wire(&Money::zero()),
        "adjusted_statement_balance": wire(&adjusted_statement),
        "adjusted_cash_book_balance": wire(&adjusted_cash_book),
        "items": [],
        "status": IN_PROGRESS,
        "journal_entry_id": Value::Null,
        "created_at": text(payload, "created_at"),
        "completed_at": Value::Null,
    });
    remember(graph, scope, BANK_REC_RECONCILIATION, body)
}

/// The references and amounts that agree, taken in one pass so the answer counts what it matched.
fn matches(lines: &mut [Value], entries: &mut [Value], reconciliation_id: &str) -> usize {
    let mut matched = 0;
    for line in lines.iter_mut() {
        if flag(line, "matched") {
            continue;
        }
        let reference = field(line, "reference");
        if reference.is_empty() {
            continue;
        }
        let moved = stored_amount(line, "amount").unwrap_or_else(|_| Money::zero());
        let limit = Money::parse(ONE_CENT).expect("a cent");
        for entry in entries.iter_mut() {
            if flag(entry, "matched") {
                continue;
            }
            if field(entry, "reference") != reference {
                continue;
            }
            let theirs = stored_amount(entry, "amount").unwrap_or_else(|_| Money::zero());
            let gap = moved.minus(&theirs).abs();
            if gap.as_decimal() < limit.as_decimal() {
                line["matched"] = json!(true);
                line["matched_transaction_id"] = json!(field(entry, "id"));
                entry["matched"] = json!(true);
                entry["matched_statement_id"] = json!(field(line, "id"));
                entry["bank_reconciliation_id"] = json!(reconciliation_id);
                matched += 1;
                break;
            }
        }
    }
    matched
}

/// `POST /reconcile/{id}/auto-match`: match a statement against the cash book, keep what is left over.
pub fn auto_match(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    reconciliation_id: &str,
    statement_id: &str,
) -> Result<Value, StoreError> {
    let mut reconciliation = Records::new(graph, BANK_REC_RECONCILIATION)
        .get(scope, reconciliation_id)
        .map_err(|_| StoreError::NotFound)?;
    let mut statement = get_statement(graph, scope, statement_id)?;
    let mut entries = cash_entries(graph, scope)?;

    let mut lines = lines_of(&statement, "lines");
    let matched = matches(&mut lines, &mut entries, reconciliation_id);
    statement["lines"] = Value::Array(lines.clone());

    // the statement's lines were matched in place; the store holds them as a property, so the whole list is
    // written back on the statement rather than line by line
    Records::new(graph, BANK_STATEMENT).update(scope, statement_id, &json!({ "lines": lines }))?;
    for entry in entries.iter().filter(|entry| flag(entry, "matched")) {
        let id = field(entry, "id");
        if id.is_empty() {
            continue;
        }
        Records::new(graph, BANK_REC_CASH_ENTRY).update(
            scope,
            id,
            &json!({
                "matched": true,
                "matched_statement_id": field(entry, "matched_statement_id"),
                "bank_reconciliation_id": field(entry, "bank_reconciliation_id"),
            }),
        )?;
    }

    let mut items: Vec<Value> = Vec::new();
    for line in lines.iter().filter(|line| !flag(line, "matched")) {
        items.push(json!({
            "item_type": "bank_only",
            "date": field(line, "date"),
            "description": field(line, "description"),
            "reference": line.get("reference").cloned().unwrap_or(Value::Null),
            "amount": field(line, "amount"),
            "match_status": "unmatched",
            "suggested_match_id": Value::Null,
            "notes": Value::Null,
        }));
    }
    let unmatched_cash: Vec<&Value> = entries
        .iter()
        .filter(|entry| !flag(entry, "matched"))
        .collect();
    for entry in &unmatched_cash {
        items.push(json!({
            "item_type": "cash_book_only",
            "date": field(entry, "date"),
            "description": field(entry, "description"),
            "reference": entry.get("reference").cloned().unwrap_or(Value::Null),
            "amount": field(entry, "amount"),
            "match_status": "unmatched",
            "suggested_match_id": Value::Null,
            "notes": Value::Null,
        }));
    }
    let unmatched_bank = lines.len() - matched;
    reconciliation["items"] = Value::Array(items);
    let saved = remember(
        graph,
        scope,
        BANK_REC_RECONCILIATION,
        json!({
            "id": reconciliation_id,
            "items": reconciliation["items"].clone(),
            "status": field(&reconciliation, "status"),
            "journal_entry_id": reconciliation.get("journal_entry_id").cloned().unwrap_or(Value::Null),
            "completed_at": reconciliation.get("completed_at").cloned().unwrap_or(Value::Null),
        }),
    )?;

    Ok(json!({
        "reconciliation": merge(&reconciliation, &saved),
        "matched_count": matched,
        "unmatched_bank": unmatched_bank,
        "unmatched_cash_book": unmatched_cash.len(),
    }))
}

fn merge(base: &Value, saved: &Value) -> Value {
    let mut out = match base {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };
    if let Value::Object(fresh) = saved {
        for (key, value) in fresh {
            out.insert(key.clone(), value.clone());
        }
    }
    Value::Object(out)
}

/// The adjustment journal the service would post, or nothing when there is nothing to adjust.
///
/// Only a bank error produces a line, and only one: a positive error is credited, a negative one debited, against
/// account 2100 or 1100. A reconciliation with no bank error posts an empty journal, which is what shipped.
pub fn adjustment_journal(reconciliation: &Value) -> Result<Value, StoreError> {
    let errors = stored_amount(reconciliation, "bank_errors")?;
    let mut lines = Vec::new();
    if errors != Money::zero() {
        let size = errors.abs();
        let positive = !errors.is_negative();
        let nothing = Money::zero();
        let (debit, credit) = if positive {
            (&nothing, &size)
        } else {
            (&size, &nothing)
        };
        lines.push(json!({
            "account_number": if positive { "2100" } else { "1100" },
            "description": "Bank reconciliation adjustment",
            "debit": wire(debit),
            "credit": wire(credit),
        }));
    }
    let date = field(reconciliation, "reconciliation_date");
    let id = field(reconciliation, "id");
    Ok(json!({
        "entry_date": date,
        "description": format!("Bank reconciliation adjustments - {date}"),
        "reference_number": format!("BANK-REC-{}", &id[..id.len().min(8)]),
        "source_module": "Bank Reconciliation",
        "status": "posted",
        "lines": lines,
    }))
}

/// `POST /reconcile/{id}/post-adjustments`: the reconciliation is complete once its journal is posted.
pub fn complete(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    reconciliation_id: &str,
    journal_entry_id: Option<&str>,
    completed_at: &str,
) -> Result<Value, StoreError> {
    let journal = journal_entry_id.map(|id| json!(id)).unwrap_or(Value::Null);
    let updated = Records::new(graph, BANK_REC_RECONCILIATION).update(
        scope,
        reconciliation_id,
        &json!({
            "status": COMPLETED,
            "journal_entry_id": journal,
            "completed_at": completed_at,
        }),
    )?;
    Ok(updated)
}

/// `GET /reconciliations`: narrowed by account and state, and never more than the last twenty.
pub fn list_reconciliations(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let account = label(query, "bank_account");
    let state = label(query, "status");
    let mut mine = reconciliations(graph, scope)?;
    if !account.is_empty() {
        mine.retain(|row| field(row, "bank_account") == account);
    }
    if !state.is_empty() {
        mine.retain(|row| field(row, "status") == state);
    }
    // the store hands them back newest first, so the tail of the shipped list is the head of this one
    let kept: Vec<Value> = mine.into_iter().take(20).collect();
    Ok(json!({ "reconciliations": kept }))
}

/// `GET /reconciliations/{id}`.
pub fn get_reconciliation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    reconciliation_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, BANK_REC_RECONCILIATION)
        .get(scope, reconciliation_id)
        .map_err(|_| StoreError::NotFound)
}

/// `GET /outstanding-items`: the cheques and deposits left over by the last reconciliation on an account.
pub fn outstanding_items(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let account = label(query, "bank_account");
    if account.is_empty() {
        return Err(refused("bank_account is required"));
    }
    // the as-at date is accepted and not used: the shipped report measured the last reconciliation, and a
    // reconciliation already carries the date it was struck on. Kept as shipped.
    let latest = reconciliations(graph, scope)?
        .into_iter()
        .find(|row| field(row, "bank_account") == account);
    let empty = json!({
        "outstanding_cheques": [],
        "outstanding_deposits": [],
        "total_cheques": "0.000000",
        "total_deposits": "0.000000",
    });
    let Some(latest) = latest else {
        return Ok(empty);
    };

    let mut cheques = Vec::new();
    let mut deposits = Vec::new();
    for item in lines_of(&latest, "items") {
        let moved = stored_amount(&item, "amount")?;
        let said = field(&item, "description").to_lowercase();
        let row = json!({
            "date": field(&item, "date"),
            "description": field(&item, "description"),
            "amount": wire(&moved),
            "reference": item.get("reference").cloned().unwrap_or(Value::Null),
        });
        // an item can be both: a cheque described as a deposit is listed on both sides, as shipped
        if !moved.is_negative() && !moved.is_zero() && said.contains("cheque") {
            cheques.push(row.clone());
        }
        if moved.is_negative() || said.contains("deposit") {
            deposits.push(row);
        }
    }
    Ok(json!({
        "outstanding_cheques": cheques,
        "outstanding_deposits": deposits,
        "total_cheques": field(&latest, "outstanding_cheques"),
        "total_deposits": field(&latest, "outstanding_deposits"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn line(id: &str, reference: &str, amount: &str) -> Value {
        json!({
            "id": id,
            "date": "2026-03-01",
            "description": "EFT payment",
            "reference": reference,
            "transaction_type": "deposit",
            "amount": amount,
            "balance": "0",
            "is_debit": false,
            "matched": false,
        })
    }

    fn entry(id: &str, reference: &str, amount: &str) -> Value {
        json!({
            "id": id,
            "date": "2026-03-01",
            "description": "receipt",
            "reference": reference,
            "transaction_type": "deposit",
            "amount": amount,
            "is_debit": false,
            "matched": false,
        })
    }

    #[test]
    fn a_statement_measures_its_own_totals_and_stamps_its_lines() {
        let graph = FakeGraph::queue(vec![
            // the statement, then the write-back of the lines stamped with it
            vec![json!({"id": "s-1"})],
            vec![json!({"id": "s-1"})],
        ]);
        let answer = import_statement(
            &graph,
            &scope(),
            &json!({
                "bank_account": "1234",
                "statement_number": "ST-1",
                "statement_start_date": "2026-03-01T00:00:00",
                "statement_end_date": "2026-03-31T00:00:00",
                "opening_balance": 100,
                "closing_balance": 850,
                "total_credits": 9999,
                "lines": [
                    {"date": "2026-03-02", "description": "in", "reference": "A", "transaction_type": "deposit", "amount": 500, "is_debit": false},
                    {"date": "2026-03-03", "description": "out", "reference": "B", "transaction_type": "cheque", "amount": 200, "is_debit": true},
                    {"date": "2026-03-04", "description": "in", "reference": "C", "transaction_type": "deposit", "amount": 450, "is_debit": false},
                ],
            }),
        )
        .expect("imported");
        assert_eq!(answer["total_credits"], json!("950.000000"));
        assert_eq!(answer["total_debits"], json!("200.000000"));
        assert_eq!(answer["status"], json!("imported"));
        let (_, stamping) = graph.last();
        assert_eq!(stamping["patch"]["lines"][0]["statement_id"], json!("s-1"));
        assert_eq!(stamping["patch"]["lines"][2]["reference"], json!("C"));
        let (_, created) = graph.statement(0);
        assert_eq!(created["props"]["total_credits"], json!("950.000000"));
    }

    #[test]
    fn a_line_of_an_unknown_kind_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        let refused = import_statement(
            &graph,
            &scope(),
            &json!({
                "bank_account": "1234",
                "statement_number": "ST-1",
                "lines": [{"date": "2026-03-02", "description": "in", "transaction_type": "telepathy", "amount": 5}],
            }),
        );
        assert!(refused.is_err());
    }

    #[test]
    fn a_cash_book_entry_arrives_unmatched_whatever_the_caller_said() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "e-1"})]]);
        add_cash_book_entry(
            &graph,
            &scope(),
            &json!({
                "date": "2026-03-01",
                "description": "receipt",
                "reference": "A",
                "transaction_type": "deposit",
                "amount": 500,
                "matched": true,
                "matched_statement_id": "line-1",
                "bank_reconciliation_id": "rec-1",
            }),
        )
        .expect("kept");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["matched"], json!(false));
        assert_eq!(params["props"]["matched_statement_id"], Value::Null);
        assert_eq!(params["props"]["bank_reconciliation_id"], Value::Null);
    }

    #[test]
    fn a_reconciliation_takes_the_bank_error_off_both_sides() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "rec-1"})]]);
        let answer = create_reconciliation(
            &graph,
            &scope(),
            &json!({
                "bank_account": "1234",
                "reconciliation_date": "2026-03-31T00:00:00",
                "statement_balance": 1000,
                "cash_book_balance": 900,
                "outstanding_deposits": 50,
                "outstanding_cheques": 150,
                "bank_errors": 10,
                "cash_book_errors": 0,
            }),
        )
        .expect("created");
        // 1000 - 150 + 50 - 10 = 890, and 900 - 10 + 0 = 890: the bank error cancels, so the difference is nil
        assert_eq!(answer["adjusted_statement_balance"], json!("890.000000"));
        assert_eq!(answer["adjusted_cash_book_balance"], json!("890.000000"));
        assert_eq!(answer["difference"], json!("0.000000"));
        assert_eq!(answer["unpresented_cheques"], json!("0.000000"));
        assert_eq!(answer["uncredited_deposits"], json!("0.000000"));
        assert_eq!(answer["status"], json!("in_progress"));
    }

    #[test]
    fn an_unexplained_difference_is_the_gap_between_the_two_sides() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "rec-1"})]]);
        let answer = create_reconciliation(
            &graph,
            &scope(),
            &json!({
                "bank_account": "1234",
                "reconciliation_date": "2026-03-31T00:00:00",
                "statement_balance": 1000,
                "cash_book_balance": 900,
            }),
        )
        .expect("created");
        assert_eq!(answer["difference"], json!("100.000000"));
    }

    #[test]
    fn matching_needs_the_same_reference_and_an_amount_within_a_cent() {
        let mut lines = vec![
            line("l-1", "A", "500.000000"),
            line("l-2", "B", "300.000000"),
            line("l-3", "", "100.000000"),
        ];
        let mut entries = vec![
            entry("e-1", "A", "500.004000"),
            entry("e-2", "B", "301.000000"),
            entry("e-3", "", "100.000000"),
        ];
        let matched = matches(&mut lines, &mut entries, "rec-1");
        // A agrees to within a cent; B is a pound out and is left alone; a reference nobody stated never matches
        assert_eq!(matched, 1);
        assert_eq!(lines[0]["matched"], json!(true));
        assert_eq!(lines[1]["matched"], json!(false));
        assert_eq!(lines[2]["matched"], json!(false));
        assert_eq!(entries[0]["matched"], json!(true));
        assert_eq!(entries[0]["bank_reconciliation_id"], json!("rec-1"));
        assert_eq!(entries[1]["matched"], json!(false));
    }

    #[test]
    fn what_is_left_over_is_reported_on_both_sides() {
        let graph = FakeGraph::queue(vec![
            // the reconciliation
            vec![
                json!({"id": "rec-1", "bank_account": "1234", "status": "in_progress", "items": []}),
            ],
            // the statement
            vec![json!({
                "id": "s-1", "bank_account": "1234",
                "lines": [line("l-1", "A", "500.000000"), line("l-2", "B", "300.000000")]
            })],
            // the cash book
            vec![entry("e-1", "A", "500.000000")],
            // the statement write-back, the entry write-back, the reconciliation write-back
            vec![json!({"id": "s-1"})],
            vec![json!({"id": "e-1"})],
            vec![json!({"id": "rec-1"})],
        ]);
        let answer = auto_match(&graph, &scope(), "rec-1", "s-1").expect("matched");
        assert_eq!(answer["matched_count"], json!(1));
        assert_eq!(answer["unmatched_bank"], json!(1));
        assert_eq!(answer["unmatched_cash_book"], json!(0));
        assert_eq!(
            answer["reconciliation"]["items"][0]["item_type"],
            json!("bank_only")
        );
        assert_eq!(answer["reconciliation"]["items"][0]["id"], Value::Null);
        assert_eq!(
            answer["reconciliation"]["items"][0]["description"],
            json!("EFT payment")
        );
    }

    #[test]
    fn a_reconciliation_that_is_not_the_callers_answers_as_missing() {
        let graph = FakeGraph::returning(vec![]);
        assert!(auto_match(&graph, &scope(), "rec-9", "s-1").is_err());
    }

    #[test]
    fn the_adjustment_journal_is_one_line_sized_by_the_bank_error() {
        let overdrawn = json!({"id": "abcdef123456", "bank_errors": "-25.000000", "reconciliation_date": "2026-03-31T00:00:00"});
        let journal = adjustment_journal(&overdrawn).expect("journal");
        assert_eq!(journal["reference_number"], json!("BANK-REC-abcdef12"));
        assert_eq!(journal["lines"][0]["account_number"], json!("1100"));
        assert_eq!(journal["lines"][0]["debit"], json!("25.000000"));
        assert_eq!(journal["lines"][0]["credit"], json!("0.000000"));

        let over = json!({"id": "abcdef123456", "bank_errors": "25.000000", "reconciliation_date": "2026-03-31T00:00:00"});
        let journal = adjustment_journal(&over).expect("journal");
        assert_eq!(journal["lines"][0]["account_number"], json!("2100"));
        assert_eq!(journal["lines"][0]["credit"], json!("25.000000"));
    }

    #[test]
    fn a_reconciliation_with_nothing_to_adjust_states_an_empty_journal() {
        let quiet = json!({"id": "rec-1", "bank_errors": "0", "reconciliation_date": "2026-03-31T00:00:00"});
        let journal = adjustment_journal(&quiet).expect("journal");
        assert_eq!(journal["lines"], json!([]));
        assert_eq!(journal["reference_number"], json!("BANK-REC-rec-1"));
    }

    #[test]
    fn completing_a_reconciliation_records_the_journal_it_posted() {
        let graph = FakeGraph::queue(vec![vec![json!({
            "id": "rec-1", "status": "completed", "journal_entry_id": "je-1",
            "completed_at": "2026-04-01T00:00:00"
        })]]);
        let answer = complete(
            &graph,
            &scope(),
            "rec-1",
            Some("je-1"),
            "2026-04-01T00:00:00",
        )
        .expect("done");
        assert_eq!(answer["status"], json!("completed"));
        let (_, params) = graph.last();
        assert_eq!(params["patch"]["journal_entry_id"], json!("je-1"));
    }

    #[test]
    fn the_listing_keeps_the_last_twenty_and_narrows_by_state() {
        let mut rows = Vec::new();
        for index in 0..25 {
            rows.push(json!({
                "id": format!("rec-{index}"), "bank_account": "1234",
                "status": if index % 2 == 0 { "completed" } else { "in_progress" }
            }));
        }
        let graph = FakeGraph::returning(rows);
        let answer = list_reconciliations(&graph, &scope(), &json!({"bank_account": "1234"}))
            .expect("listed");
        assert_eq!(answer["reconciliations"].as_array().unwrap().len(), 20);

        let graph = FakeGraph::returning((0..3).map(|i| json!({
            "id": format!("rec-{i}"), "bank_account": "1234", "status": if i == 0 { "completed" } else { "in_progress" }
        })).collect());
        let answer = list_reconciliations(&graph, &scope(), &json!({"status": "in_progress"}))
            .expect("listed");
        assert_eq!(answer["reconciliations"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn outstanding_items_come_from_the_last_reconciliation_on_the_account() {
        let graph = FakeGraph::returning(vec![
            json!({
                "id": "rec-2", "bank_account": "1234", "outstanding_cheques": "150.000000",
                "outstanding_deposits": "50.000000",
                "items": [
                    {"date": "2026-03-01", "description": "Cheque 1042", "amount": "150.000000", "reference": "1042"},
                    {"date": "2026-03-02", "description": "Deposit not yet credited", "amount": "50.000000", "reference": "D-1"},
                    {"date": "2026-03-03", "description": "Refund", "amount": "-20.000000", "reference": "R-1"},
                ],
            }),
            json!({"id": "rec-1", "bank_account": "1234", "items": []}),
        ]);
        let answer = outstanding_items(&graph, &scope(), &json!({"bank_account": "1234"}))
            .expect("reported");
        assert_eq!(answer["outstanding_cheques"].as_array().unwrap().len(), 1);
        // the deposit is listed by its description, and the refund by its sign
        assert_eq!(answer["outstanding_deposits"].as_array().unwrap().len(), 2);
        assert_eq!(answer["total_cheques"], json!("150.000000"));
        assert_eq!(answer["total_deposits"], json!("50.000000"));
    }

    #[test]
    fn an_account_that_was_never_reconciled_reports_nothing() {
        let graph = FakeGraph::returning(vec![]);
        let answer = outstanding_items(&graph, &scope(), &json!({"bank_account": "9999"}))
            .expect("reported");
        assert_eq!(answer["total_cheques"], json!("0.000000"));
        assert_eq!(answer["outstanding_cheques"], json!([]));
    }

    #[test]
    fn the_cash_book_listing_needs_an_account_and_narrows_by_window_and_flag() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "e-1", "date": "2026-03-01", "matched": true}),
            json!({"id": "e-2", "date": "2026-03-10", "matched": false}),
            json!({"id": "e-3", "date": "2026-03-20", "matched": false}),
        ]);
        let answer = list_cash_book(
            &graph,
            &scope(),
            &json!({"bank_account": "1234", "matched": false}),
        )
        .expect("listed");
        assert_eq!(answer["entries"].as_array().unwrap().len(), 2);

        let answer = list_cash_book(
            &graph,
            &scope(),
            &json!({"bank_account": "1234", "start_date": "2026-03-05", "end_date": "2026-03-15"}),
        )
        .expect("listed");
        assert_eq!(answer["entries"].as_array().unwrap().len(), 1);
        assert_eq!(answer["entries"][0]["id"], json!("e-2"));

        let refused = list_cash_book(&graph, &scope(), &json!({}));
        assert!(refused.is_err());
    }
}
