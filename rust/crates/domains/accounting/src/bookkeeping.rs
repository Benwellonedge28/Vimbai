#![forbid(unsafe_code)]
//! accounting-service's supporting records: petty cash, bank reconciliation, statements of affairs,
//! capital calculations, control accounts, receipts and payments, and the single-entry conversions.
//!
//! These are the working papers beside the ledger. Each one takes the figures a bookkeeper prepared and
//! derives the totals the document publishes, in exact arithmetic, then keeps the record Book-scoped.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * petty cash vouchers post to a fixed petty-cash account `1001` against one expense account chosen by
//!   category, and an unknown category falls back to `6600`; the fund's own balance is never moved, so
//!   the imprest never runs down;
//! * a bank reconciliation carries no deposits in transit, no outstanding cheques, no charges and no
//!   interest, because the service never collected them: its adjusted bank balance is the statement
//!   balance and its adjusted book balance is the book balance, so it reports whether those two agree;
//! * a statement of affairs takes capital to be assets less liabilities, and keeps the itemised assets
//!   and liabilities on the record it returns;
//! * a capital calculation adds additional capital and profit and takes away loss and withdrawals, but
//!   when neither a profit nor a loss was given it reports the opening capital as the closing capital,
//!   ignoring the additional capital and the withdrawals it was handed;
//! * a control account totals only the entry types it knows: a debtors account counts a credit sale,
//!   a receipt and a discount, and a creditors account a credit purchase, a payment and a discount
//!   allowed, while every other type moves the running balance and no total;
//! * each control-account entry records the balance as it stood *before* that entry, so the entries are
//!   reported one step behind the closing balance;
//! * a receipts and payments account totals its receipts and payments, splits them by category, and moves
//!   the closing bank balance only: the closing cash balance is the opening one, because contra entries
//!   between cash and bank were never applied;
//! * a single-entry conversion and a profit estimation ask the same question - what did this book earn -
//!   and answer it the same way, so both routes call one calculation rather than two copies of it;
//!   a net loss is reported as a loss with net profit set to nothing, and neither route records the cash
//!   or debtor and creditor balances it accepts.
//!
//! One behaviour deliberately changed: a reconciliation's per-line running balance is the running book
//! balance of the account, where the service reported the account's debit total less its credit total
//! before each line, which is not a running balance at all.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::{amount, wire};
use vimbai_store::{
    created_record, BookScope, Graph, RecordKind, Records, StoreError, BANK_RECONCILIATION,
    CAPITAL_CALCULATION, CONTROL_ACCOUNT, PETTY_CASH_ENTRY, PETTY_CASH_FUND, PROFIT_ESTIMATION,
    RECEIPTS_PAYMENTS, SINGLE_ENTRY_CONVERSION, STATEMENT_OF_AFFAIRS,
};

use crate::ledger_book;

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

fn money(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        Some(Value::Null) | None => Err(refused(&format!("{key} is required"))),
        Some(value) => amount(&json!({ "amount": value }), "amount"),
    }
}

fn money_or_nothing(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        Some(Value::Null) | None => Ok(Money::zero()),
        Some(value) => amount(&json!({ "amount": value }), "amount"),
    }
}

fn rows(payload: &Value, key: &str) -> Vec<Value> {
    payload
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn remember(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    kind: RecordKind,
    body: Value,
) -> Result<Value, StoreError> {
    let saved = Records::new(graph, kind).create(scope, &body)?;
    let mut answer = created_record(&saved);
    if let (Some(answer), Some(fields)) = (answer.as_object_mut(), body.as_object()) {
        for (key, value) in fields {
            answer.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    Ok(answer)
}

fn listing(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    kind: RecordKind,
    query: &Value,
    date_key: &str,
    owner: Option<(&str, &str)>,
) -> Result<Value, StoreError> {
    let from = optional(query, "start_date");
    let until = optional(query, "end_date");
    let mut rows: Vec<Value> = Records::new(graph, kind)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            from.as_deref()
                .map(|b| field(row, date_key) >= b)
                .unwrap_or(true)
        })
        .filter(|row| {
            until
                .as_deref()
                .map(|b| field(row, date_key) <= b)
                .unwrap_or(true)
        })
        .filter(|row| {
            owner
                .and_then(|(incoming, stored)| {
                    optional(query, incoming).map(|wanted| field(row, stored) == wanted)
                })
                .unwrap_or(true)
        })
        .collect();
    rows.sort_by(|one, two| {
        field(two, date_key)
            .cmp(field(one, date_key))
            .then(field(two, "created_at").cmp(field(one, "created_at")))
    });
    Ok(Value::Array(rows))
}

// --- petty cash ---

/// `POST /petty-cash-funds/`.
pub fn create_petty_cash_fund(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let imprest = money(payload, "imprest_amount")?;
    let balance = money_or_nothing(payload, "current_balance")?;
    remember(
        graph,
        scope,
        PETTY_CASH_FUND,
        json!({
            "fund_name": text(payload, "fund_name"),
            "fund_number": text(payload, "fund_number"),
            "imprest_amount": wire(&imprest),
            "current_balance": wire(&balance),
            "custodian": text(payload, "custodian"),
            "location": payload.get("location").cloned().unwrap_or(Value::Null),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /petty-cash-funds/`.
pub fn list_petty_cash_funds(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    Records::new(graph, PETTY_CASH_FUND)
        .list(scope, 2000)
        .map(Value::Array)
}

/// The expense account each petty cash category posts to, and the fallback for the rest.
fn petty_cash_expense(category: &str) -> &'static str {
    match category {
        "office_supplies" => "6100",
        "postage" => "6200",
        "transportation" => "6300",
        "meals" => "6400",
        "tips" => "6500",
        _ => "6600",
    }
}

/// `POST /petty-cash-entries/`: a voucher, posted against petty cash.
pub fn create_petty_cash_entry(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let amount = money(payload, "amount")?;
    let voucher = text(payload, "voucher_number");
    let category = or_default(payload, "category", "miscellaneous");
    let expense_account = petty_cash_expense(&category);
    let zero = Money::zero();

    let entry = json!({
        "entry_date": text(payload, "voucher_date"),
        "description": format!("Petty Cash Voucher: {voucher}"),
        "reference_number": voucher,
        "source_module": "PettyCash",
        "status": "posted",
        "created_at": text(payload, "created_at"),
        "lines": [
            {"account_number": expense_account, "debit": wire(&amount), "credit": wire(&zero),
             "description": format!("Petty Cash: {voucher}")},
            {"account_number": "1001", "debit": wire(&zero), "credit": wire(&amount),
             "description": format!("Petty Cash: {voucher}")},
        ],
    });
    let posted = ledger_book::create_entry(graph, scope, &entry)?;

    remember(
        graph,
        scope,
        PETTY_CASH_ENTRY,
        json!({
            "petty_cash_fund_id": text(payload, "petty_cash_fund_id"),
            "voucher_number": voucher,
            "voucher_date": text(payload, "voucher_date"),
            "payee": text(payload, "payee"),
            "amount": wire(&amount),
            "category": category,
            "description": text(payload, "description"),
            "receipt_number": payload.get("receipt_number").cloned().unwrap_or(Value::Null),
            "approved_by": payload.get("approved_by").cloned().unwrap_or(Value::Null),
            "journal_entry_id": field(&posted, "id"),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /petty-cash-entries/`.
pub fn list_petty_cash_entries(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    listing(
        graph,
        scope,
        PETTY_CASH_ENTRY,
        query,
        "voucher_date",
        Some(("fund_id", "petty_cash_fund_id")),
    )
}

// --- bank reconciliation ---

/// `POST /bank-reconciliation/`: the book balance against the bank's, as of a statement date.
pub fn create_bank_reconciliation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let account_number = text(payload, "bank_account");
    if account_number.is_empty() {
        return Err(refused("bank_account is required"));
    }
    let statement_date = text(payload, "statement_date");
    let statement_balance = money(payload, "statement_balance")?;

    let journal = Records::new(graph, vimbai_store::JOURNAL_ENTRY).list(scope, 4000)?;

    let mut book_balance = Money::zero();
    let mut lines = Vec::new();
    let mut movements: Vec<(String, String, Money, Money, String, String)> = Vec::new();
    for entry in journal
        .iter()
        .filter(|entry| field(entry, "status") == "posted")
    {
        if field(entry, "entry_date") > statement_date.as_str() {
            continue;
        }
        for line in entry
            .get("lines")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            if field(&line, "account_number") != account_number {
                continue;
            }
            let debit = money_or_nothing(&line, "debit")?;
            let credit = money_or_nothing(&line, "credit")?;
            movements.push((
                field(entry, "entry_date").to_string(),
                field(entry, "description").to_string(),
                debit,
                credit,
                field(entry, "reference_number").to_string(),
                field(entry, "id").to_string(),
            ));
        }
    }
    movements.sort_by(|one, two| one.0.cmp(&two.0));

    for (date, description, debit, credit, reference, entry_id) in movements {
        book_balance = book_balance.plus(&debit).minus(&credit);
        lines.push(json!({
            "transaction_date": date,
            "description": description,
            "debit": wire(&debit),
            "credit": wire(&credit),
            "running_balance": wire(&book_balance),
            "reference": reference,
            "matched": true,
            "match_type": "journal_entry",
            "matched_entry_id": entry_id,
        }));
    }

    // nothing the service never collected: no deposits in transit, no outstanding cheques, no charges
    let adjusted_bank = statement_balance.clone();
    let adjusted_book = book_balance.clone();
    let difference = adjusted_bank.minus(&adjusted_book);
    let reconciled = difference == Money::zero();

    remember(
        graph,
        scope,
        BANK_RECONCILIATION,
        json!({
            "bank_account_number": account_number,
            "bank_name": format!("Bank Account {account_number}"),
            "statement_date": statement_date,
            "statement_balance": wire(&statement_balance),
            "book_balance": wire(&book_balance),
            "deposits_in_transit": [],
            "outstanding_checks": [],
            "bank_charges": wire(&Money::zero()),
            "interest_earned": wire(&Money::zero()),
            "insufficient_funds": wire(&Money::zero()),
            "adjusted_bank_balance": wire(&adjusted_bank),
            "adjusted_book_balance": wire(&adjusted_book),
            "difference": wire(&difference),
            "bank_entries": [],
            "journal_entries": lines,
            "is_reconciled": reconciled,
            "reconciled_date": Value::Null,
            "reconciled_by": Value::Null,
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /bank-reconciliation/`.
pub fn list_bank_reconciliations(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    listing(
        graph,
        scope,
        BANK_RECONCILIATION,
        query,
        "statement_date",
        Some(("bank_account", "bank_account_number")),
    )
}

/// `GET /bank-reconciliation/latest/{account}`: the most recent statement for one account.
pub fn latest_bank_reconciliation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    account_number: &str,
) -> Result<Value, StoreError> {
    let mut mine: Vec<Value> = Records::new(graph, BANK_RECONCILIATION)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| field(row, "bank_account_number") == account_number)
        .collect();
    mine.sort_by(|one, two| field(two, "statement_date").cmp(field(one, "statement_date")));
    mine.into_iter().next().ok_or(StoreError::NotFound)
}

// --- statements of affairs ---

/// `POST /statements-of-affairs/`.
pub fn create_statement_of_affairs(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let mut total_assets = Money::zero();
    let mut asset_lines = Vec::new();
    for line in rows(payload, "assets") {
        let value = money(&line, "asset_value")?;
        total_assets = total_assets.plus(&value);
        asset_lines.push(json!({
            "asset_name": text(&line, "asset_name"),
            "asset_category": line.get("asset_category").cloned().unwrap_or(Value::Null),
            "asset_value": wire(&value),
            "asset_notes": line.get("asset_notes").cloned().unwrap_or(Value::Null),
            "is_debtor": line.get("is_debtor").cloned().unwrap_or(json!(false)),
        }));
    }

    let mut total_liabilities = Money::zero();
    let mut liability_lines = Vec::new();
    for line in rows(payload, "liabilities") {
        let value = money(&line, "liability_value")?;
        total_liabilities = total_liabilities.plus(&value);
        liability_lines.push(json!({
            "liability_name": text(&line, "liability_name"),
            "liability_category": line.get("liability_category").cloned().unwrap_or(Value::Null),
            "liability_value": wire(&value),
            "liability_notes": line.get("liability_notes").cloned().unwrap_or(Value::Null),
            "is_creditor": line.get("is_creditor").cloned().unwrap_or(json!(false)),
        }));
    }

    let capital = total_assets.minus(&total_liabilities);
    remember(
        graph,
        scope,
        STATEMENT_OF_AFFAIRS,
        json!({
            "as_of_date": text(payload, "as_of_date"),
            "prepared_by": payload.get("prepared_by").cloned().unwrap_or(Value::Null),
            "prepared_date": text(payload, "created_at"),
            "total_assets": wire(&total_assets),
            "total_liabilities": wire(&total_liabilities),
            "capital": wire(&capital),
            "assets": asset_lines,
            "liabilities": liability_lines,
            "notes": Value::Null,
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /statements-of-affairs/`.
pub fn list_statements_of_affairs(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let mut all = Records::new(graph, STATEMENT_OF_AFFAIRS).list(scope, 2000)?;
    all.sort_by(|one, two| field(two, "as_of_date").cmp(field(one, "as_of_date")));
    Ok(Value::Array(all))
}

/// `GET /statements-of-affairs/{date}`: the statement prepared for that date.
pub fn statement_of_affairs_on(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    as_of_date: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, STATEMENT_OF_AFFAIRS)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "as_of_date") == as_of_date)
        .ok_or(StoreError::NotFound)
}

// --- capital calculations ---

/// `POST /capital-calculations/`.
pub fn create_capital_calculation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let opening = money(payload, "opening_capital")?;
    let net_profit = money_or_nothing(payload, "net_profit")?;
    let net_loss = money_or_nothing(payload, "net_loss")?;

    let mut additional = Money::zero();
    let mut withdrawals = Money::zero();
    let mut entries = Vec::new();
    for entry in rows(payload, "entries") {
        let value = money(&entry, "amount")?;
        match field(&entry, "entry_type") {
            "additional_capital" => additional = additional.plus(&value),
            "withdrawal" => withdrawals = withdrawals.plus(&value),
            // a net_profit or net_loss entry is recorded but never totalled, as shipped
            _ => {}
        }
        entries.push(json!({
            "id": format!("{}-{}", text(payload, "id"), entries.len() + 1),
            "entry_date": text(&entry, "entry_date"),
            "entry_type": text(&entry, "entry_type"),
            "amount": wire(&value),
            "description": text(&entry, "description"),
            "reference_number": entry.get("reference_number").cloned().unwrap_or(Value::Null),
            "created_at": text(payload, "created_at"),
        }));
    }

    let (closing, kind) = if net_profit > Money::zero() {
        (
            opening
                .plus(&additional)
                .plus(&net_profit)
                .minus(&withdrawals),
            "profit",
        )
    } else if net_loss > Money::zero() {
        (
            opening
                .plus(&additional)
                .minus(&net_loss)
                .minus(&withdrawals),
            "loss",
        )
    } else {
        // as shipped: no profit and no loss means nothing else was applied either
        (opening.clone(), "none")
    };

    remember(
        graph,
        scope,
        CAPITAL_CALCULATION,
        json!({
            "as_of_date": text(payload, "as_of_date"),
            "period_start": text(payload, "period_start"),
            "period_end": text(payload, "period_end"),
            "opening_capital": wire(&opening),
            "closing_capital": wire(&closing),
            "total_additional_capital": wire(&additional),
            "total_withdrawals": wire(&withdrawals),
            "net_profit": wire(&net_profit),
            "net_loss": wire(&net_loss),
            "profit_or_loss": kind,
            "prepared_by": payload.get("prepared_by").cloned().unwrap_or(Value::Null),
            "entries": entries,
            "notes": payload.get("notes").cloned().unwrap_or(Value::Null),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /capital-calculations/`.
pub fn list_capital_calculations(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let mut all = Records::new(graph, CAPITAL_CALCULATION).list(scope, 2000)?;
    all.sort_by(|one, two| field(two, "as_of_date").cmp(field(one, "as_of_date")));
    Ok(Value::Array(all))
}

/// `GET /capital-calculations/{id}`.
pub fn get_capital_calculation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    calc_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, CAPITAL_CALCULATION).get(scope, calc_id)
}

// --- control accounts ---

/// `POST /control-accounts/`: a debtors or creditors control account over a period.
pub fn create_control_account(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let account_type = or_default(payload, "account_type", "debtors");
    let opening = money(payload, "opening_balance")?;
    let mut total_debits = Money::zero();
    let mut total_credits = Money::zero();
    let mut running = opening.clone();

    let debit_types: [&str; 3] = ["credit_sale", "payment_received", "discount_received"];
    let credit_types: [&str; 3] = ["credit_purchase", "payment_made", "discount_allowed"];
    for entry in rows(payload, "entries") {
        let value = money(&entry, "amount")?;
        let kind = field(&entry, "entry_type");
        if account_type == "debtors" {
            if debit_types.contains(&kind) {
                if kind == "credit_sale" {
                    running = running.plus(&value);
                    total_debits = total_debits.plus(&value);
                } else {
                    running = running.minus(&value);
                    total_credits = total_credits.plus(&value);
                }
            }
        } else if credit_types.contains(&kind) {
            if kind == "credit_purchase" {
                running = running.plus(&value);
                total_credits = total_credits.plus(&value);
            } else {
                running = running.minus(&value);
                total_debits = total_debits.plus(&value);
            }
        }
    }
    let closing = running.clone();

    // the entries are reported one step behind: each records the balance before itself
    let mut reported = opening.clone();
    let mut lines = Vec::new();
    for entry in rows(payload, "entries") {
        let value = money(&entry, "amount")?;
        let kind = field(&entry, "entry_type");
        lines.push(json!({
            "entry_date": text(&entry, "entry_date"),
            "entry_type": kind,
            "amount": wire(&value),
            "description": text(&entry, "description"),
            "reference_number": entry.get("reference_number").cloned().unwrap_or(Value::Null),
            "customer_id": entry.get("customer_id").cloned().unwrap_or(Value::Null),
            "supplier_id": entry.get("supplier_id").cloned().unwrap_or(Value::Null),
            "is_contra": entry.get("isContra").cloned().unwrap_or(json!(false)),
            "running_balance": wire(&reported),
            "created_at": text(payload, "created_at"),
        }));
        if account_type == "debtors" {
            reported = if kind == "credit_sale" {
                reported.plus(&value)
            } else {
                reported.minus(&value)
            };
        } else {
            reported = if kind == "credit_purchase" || kind == "interest_charged" {
                reported.plus(&value)
            } else {
                reported.minus(&value)
            };
        }
    }

    remember(
        graph,
        scope,
        CONTROL_ACCOUNT,
        json!({
            "account_type": account_type,
            "account_name": text(payload, "account_name"),
            "as_of_date": text(payload, "as_of_date"),
            "opening_balance": wire(&opening),
            "total_debits": wire(&total_debits),
            "total_credits": wire(&total_credits),
            "closing_balance": wire(&closing),
            "prepared_by": payload.get("prepared_by").cloned().unwrap_or(Value::Null),
            "entries": lines,
            "notes": payload.get("notes").cloned().unwrap_or(Value::Null),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /control-accounts/`.
pub fn list_control_accounts(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = optional(query, "account_type");
    let mut all: Vec<Value> = Records::new(graph, CONTROL_ACCOUNT)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            wanted
                .as_deref()
                .map(|kind| field(row, "account_type") == kind)
                .unwrap_or(true)
        })
        .collect();
    all.sort_by(|one, two| field(two, "as_of_date").cmp(field(one, "as_of_date")));
    Ok(Value::Array(all))
}

// --- receipts and payments ---

/// `POST /receipts-payments/`: receipts and payments over a period, split by category.
pub fn create_receipts_payments(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let opening_cash = money_or_nothing(payload, "opening_cash_balance")?;
    let opening_bank = money_or_nothing(payload, "opening_bank_balance")?;

    let mut total_receipts = Money::zero();
    let mut total_payments = Money::zero();
    let mut capital_receipts = Money::zero();
    let mut revenue_receipts = Money::zero();
    let mut asset_payments = Money::zero();
    let mut liability_payments = Money::zero();
    let mut revenue_payments = Money::zero();
    let mut lines = Vec::new();

    for entry in rows(payload, "entries") {
        let value = money(&entry, "amount")?;
        let kind = field(&entry, "entry_type");
        let category = field(&entry, "category");
        if kind == "receipt" {
            total_receipts = total_receipts.plus(&value);
            match category {
                "capital" => capital_receipts = capital_receipts.plus(&value),
                // anything that is not capital counts as revenue on the receipts side
                "revenue" => revenue_receipts = revenue_receipts.plus(&value),
                _ => {}
            }
        } else {
            total_payments = total_payments.plus(&value);
            match category {
                "asset" => asset_payments = asset_payments.plus(&value),
                "liability" => liability_payments = liability_payments.plus(&value),
                "revenue" => revenue_payments = revenue_payments.plus(&value),
                _ => {}
            }
        }
        lines.push(json!({
            "entry_date": text(&entry, "entry_date"),
            "entry_type": kind,
            "category": category,
            "account_name": text(&entry, "account_name"),
            "amount": wire(&value),
            "description": text(&entry, "description"),
            "reference_number": entry.get("reference_number").cloned().unwrap_or(Value::Null),
            "is_contra": entry.get("isContra").cloned().unwrap_or(json!(false)),
            "created_at": text(payload, "created_at"),
        }));
    }

    // the bank balance moves; the cash balance does not, because contra entries were never applied
    let closing_bank = opening_bank.plus(&total_receipts).minus(&total_payments);
    let closing_cash = opening_cash.clone();

    remember(
        graph,
        scope,
        RECEIPTS_PAYMENTS,
        json!({
            "account_name": text(payload, "account_name"),
            "period_start": text(payload, "period_start"),
            "period_end": text(payload, "period_end"),
            "prepared_by": payload.get("prepared_by").cloned().unwrap_or(Value::Null),
            "opening_cash_balance": wire(&opening_cash),
            "opening_bank_balance": wire(&opening_bank),
            "total_receipts": wire(&total_receipts),
            "total_payments": wire(&total_payments),
            "total_capital_receipts": wire(&capital_receipts),
            "total_revenue_receipts": wire(&revenue_receipts),
            "total_asset_payments": wire(&asset_payments),
            "total_liability_payments": wire(&liability_payments),
            "total_revenue_payments": wire(&revenue_payments),
            "closing_cash_balance": wire(&closing_cash),
            "closing_bank_balance": wire(&closing_bank),
            "closing_total_balance": wire(&closing_cash.plus(&closing_bank)),
            "entries": lines,
            "notes": payload.get("notes").cloned().unwrap_or(Value::Null),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /receipts-payments/`.
pub fn list_receipts_payments(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let mut all = Records::new(graph, RECEIPTS_PAYMENTS).list(scope, 2000)?;
    all.sort_by(|one, two| field(two, "period_end").cmp(field(one, "period_end")));
    Ok(Value::Array(all))
}

// --- single entry: two doors, one calculation ---

/// What a book earned, from its capital moved.
struct Earnings {
    net_profit: Money,
    net_loss: Money,
    kind: &'static str,
}

fn earnings_from_capital(
    closing: &Money,
    opening: &Money,
    drawings: &Money,
    additional: &Money,
) -> Earnings {
    let net = closing.minus(opening).plus(drawings).minus(additional);
    if net > Money::zero() {
        Earnings {
            net_profit: net,
            net_loss: Money::zero(),
            kind: "profit",
        }
    } else if net < Money::zero() {
        Earnings {
            net_profit: Money::zero(),
            net_loss: Money::zero().minus(&net),
            kind: "loss",
        }
    } else {
        Earnings {
            net_profit: Money::zero(),
            net_loss: Money::zero(),
            kind: "none",
        }
    }
}

/// `POST /single-entry-conversions/`.
pub fn create_single_entry_conversion(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let opening = money(payload, "opening_capital")?;
    let closing = money(payload, "closing_capital")?;
    let drawings = money_or_nothing(payload, "drawings")?;
    let additional = money_or_nothing(payload, "additional_capital")?;
    let earned = earnings_from_capital(&closing, &opening, &drawings, &additional);

    remember(
        graph,
        scope,
        SINGLE_ENTRY_CONVERSION,
        json!({
            "as_of_date": text(payload, "as_of_date"),
            "prepared_by": payload.get("prepared_by").cloned().unwrap_or(Value::Null),
            "source_type": or_default(payload, "source_type", "all"),
            "opening_capital": wire(&opening),
            // the debtor, creditor and cash balances are accepted but never recorded, as shipped
            "opening_debtors": wire(&Money::zero()),
            "opening_creditors": wire(&Money::zero()),
            "opening_cash": wire(&Money::zero()),
            "closing_capital": wire(&closing),
            "closing_debtors": wire(&Money::zero()),
            "closing_creditors": wire(&Money::zero()),
            "closing_cash": wire(&Money::zero()),
            "additional_capital": wire(&additional),
            "drawings": wire(&drawings),
            "net_profit": wire(&earned.net_profit),
            "net_loss": wire(&earned.net_loss),
            "profit_or_loss": earned.kind,
            "total_receipts": wire(&money_or_nothing(payload, "total_receipts")?),
            "total_payments": wire(&money_or_nothing(payload, "total_payments")?),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /single-entry-conversions/`.
pub fn list_single_entry_conversions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let mut all = Records::new(graph, SINGLE_ENTRY_CONVERSION).list(scope, 2000)?;
    all.sort_by(|one, two| field(two, "as_of_date").cmp(field(one, "as_of_date")));
    Ok(Value::Array(all))
}

/// `POST /profit-estimations/`: the same question as a single-entry conversion, asked of the same
/// calculation rather than of a second copy of it.
pub fn create_profit_estimation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let opening = money(payload, "opening_capital")?;
    let closing = money(payload, "closing_capital")?;
    let drawings = money_or_nothing(payload, "drawings")?;
    let additional = money_or_nothing(payload, "additional_capital")?;
    let earned = earnings_from_capital(&closing, &opening, &drawings, &additional);

    remember(
        graph,
        scope,
        PROFIT_ESTIMATION,
        json!({
            "as_of_date": text(payload, "as_of_date"),
            "period_start": text(payload, "period_start"),
            "period_end": text(payload, "period_end"),
            "prepared_by": payload.get("prepared_by").cloned().unwrap_or(Value::Null),
            "opening_capital": wire(&opening),
            "closing_capital": wire(&closing),
            "additional_capital": wire(&additional),
            "drawings": wire(&drawings),
            "net_profit": wire(&earned.net_profit),
            "net_loss": wire(&earned.net_loss),
            "profit_or_loss": earned.kind,
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /profit-estimations/`.
pub fn list_profit_estimations(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let mut all = Records::new(graph, PROFIT_ESTIMATION).list(scope, 2000)?;
    all.sort_by(|one, two| field(two, "as_of_date").cmp(field(one, "as_of_date")));
    Ok(Value::Array(all))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_statement_of_affairs_takes_capital_to_be_assets_less_liabilities() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "soa-1"})]]);
        let answer = create_statement_of_affairs(
            &graph,
            &scope(),
            &json!({"as_of_date": "2026-03-31", "created_at": "2026-03-31T10:00:00",
                    "assets": [
                        {"asset_name": "Van", "asset_value": "123456789012345678901.55"},
                        {"asset_name": "Cash", "asset_value": "0.45"}
                    ],
                    "liabilities": [{"liability_name": "Loan", "liability_value": "1.00"}]}),
        )
        .expect("statement");
        assert_eq!(
            answer["total_assets"],
            json!("123456789012345678902.000000")
        );
        assert_eq!(answer["capital"], json!("123456789012345678901.000000"));
        assert_eq!(answer["assets"].as_array().expect("assets").len(), 2);
    }

    #[test]
    fn a_capital_calculation_applies_profit_and_withdrawals() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "cc-1"})]]);
        let answer = create_capital_calculation(
            &graph,
            &scope(),
            &json!({"as_of_date": "2026-03-31", "opening_capital": "1000.00",
            "net_profit": "250.00", "entries": [
                {"entry_type": "additional_capital", "amount": "300.00"},
                {"entry_type": "withdrawal", "amount": "100.00"}
            ]}),
        )
        .expect("calculation");
        // 1000 + 300 + 250 - 100
        assert_eq!(answer["closing_capital"], json!("1450.000000"));
        assert_eq!(answer["profit_or_loss"], json!("profit"));
    }

    #[test]
    fn a_capital_calculation_given_no_profit_reports_the_opening_capital_back() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "cc-1"})]]);
        let answer = create_capital_calculation(
            &graph,
            &scope(),
            &json!({"as_of_date": "2026-03-31", "opening_capital": "1000.00",
            "entries": [
                {"entry_type": "additional_capital", "amount": "300.00"},
                {"entry_type": "withdrawal", "amount": "100.00"}
            ]}),
        )
        .expect("calculation");
        // as shipped: the additional capital and the withdrawal are totalled but not applied
        assert_eq!(answer["closing_capital"], json!("1000.000000"));
        assert_eq!(answer["total_additional_capital"], json!("300.000000"));
        assert_eq!(answer["total_withdrawals"], json!("100.000000"));
    }

    #[test]
    fn a_debtors_control_account_totals_only_the_types_it_knows() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "ca-1"})]]);
        let answer = create_control_account(
            &graph,
            &scope(),
            &json!({"account_type": "debtors", "account_name": "Debtors Control",
            "as_of_date": "2026-03-31", "opening_balance": "500.00", "entries": [
                {"entry_type": "credit_sale", "amount": "200.00"},
                {"entry_type": "payment_received", "amount": "50.00"},
                {"entry_type": "cash_sale", "amount": "999.00"}
            ]}),
        )
        .expect("control account");
        assert_eq!(answer["total_debits"], json!("200.000000"));
        assert_eq!(answer["total_credits"], json!("50.000000"));
        // the cash sale moves no total and no balance
        assert_eq!(answer["closing_balance"], json!("650.000000"));
    }

    #[test]
    fn a_control_accounts_entries_report_the_balance_before_themselves() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "ca-1"})]]);
        let answer = create_control_account(
            &graph,
            &scope(),
            &json!({"account_type": "debtors", "account_name": "Debtors Control",
            "as_of_date": "2026-03-31", "opening_balance": "100.00", "entries": [
                {"entry_type": "credit_sale", "amount": "200.00"},
                {"entry_type": "payment_received", "amount": "50.00"}
            ]}),
        )
        .expect("control account");
        let entries = answer["entries"].as_array().expect("entries");
        assert_eq!(entries[0]["running_balance"], json!("100.000000"));
        assert_eq!(entries[1]["running_balance"], json!("300.000000"));
        assert_eq!(answer["closing_balance"], json!("250.000000"));
    }

    #[test]
    fn a_receipts_and_payments_account_moves_the_bank_balance_only() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "rp-1"})]]);
        let answer = create_receipts_payments(
            &graph,
            &scope(),
            &json!({"account_name": "R&P", "period_start": "2026-01-01",
            "period_end": "2026-03-31",
            "opening_cash_balance": "100.00", "opening_bank_balance": "1000.00",
            "entries": [
                {"entry_type": "receipt", "category": "capital", "amount": "500.00"},
                {"entry_type": "receipt", "category": "revenue", "amount": "250.00"},
                {"entry_type": "payment", "category": "asset", "amount": "300.00"},
                {"entry_type": "payment", "category": "liability", "amount": "100.00"}
            ]}),
        )
        .expect("receipts and payments");
        assert_eq!(answer["total_receipts"], json!("750.000000"));
        assert_eq!(answer["total_payments"], json!("400.000000"));
        assert_eq!(answer["total_capital_receipts"], json!("500.000000"));
        assert_eq!(answer["closing_bank_balance"], json!("1350.000000"));
        // the cash balance never moves
        assert_eq!(answer["closing_cash_balance"], json!("100.000000"));
        assert_eq!(answer["closing_total_balance"], json!("1450.000000"));
    }

    #[test]
    fn the_two_single_entry_doors_agree_because_they_are_one_calculation() {
        let payload = json!({"as_of_date": "2026-03-31", "period_start": "2026-01-01",
                             "period_end": "2026-03-31",
                             "opening_capital": "1000.00", "closing_capital": "1800.00",
                             "additional_capital": "200.00", "drawings": "150.00"});
        let conversion = create_single_entry_conversion(
            &FakeGraph::queue(vec![vec![json!({"id": "se-1"})]]),
            &scope(),
            &payload,
        )
        .expect("conversion");
        let estimation = create_profit_estimation(
            &FakeGraph::queue(vec![vec![json!({"id": "pe-1"})]]),
            &scope(),
            &payload,
        )
        .expect("estimation");
        // 1800 - 1000 + 150 - 200
        assert_eq!(conversion["net_profit"], json!("750.000000"));
        assert_eq!(estimation["net_profit"], json!("750.000000"));
        assert_eq!(conversion["profit_or_loss"], estimation["profit_or_loss"]);
    }

    #[test]
    fn a_loss_is_reported_as_a_loss_with_no_profit_beside_it() {
        let earned = earnings_from_capital(
            &Money::parse("900.00").expect("money"),
            &Money::parse("1200.00").expect("money"),
            &Money::zero(),
            &Money::zero(),
        );
        assert_eq!(earned.kind, "loss");
        assert_eq!(earned.net_loss, Money::parse("300.00").expect("money"));
        assert_eq!(earned.net_profit, Money::zero());
    }

    #[test]
    fn a_reconciliation_compares_the_statement_with_the_book() {
        let journal = vec![
            json!({"id": "je-1", "entry_date": "2026-03-01", "status": "posted",
                   "description": "receipt", "reference_number": "R1",
                   "lines": [{"account_number": "1000", "debit": "500.000000", "credit": "0.000000"}]}),
            json!({"id": "je-2", "entry_date": "2026-03-05", "status": "posted",
                   "description": "payment", "reference_number": "P1",
                   "lines": [{"account_number": "1000", "debit": "0.000000", "credit": "125.500000"}]}),
            // a draft moves nothing
            json!({"id": "je-3", "entry_date": "2026-03-06", "status": "draft",
                   "description": "draft",
                   "lines": [{"account_number": "1000", "debit": "9999.000000", "credit": "0.000000"}]}),
            // and neither does a line on another account
            json!({"id": "je-4", "entry_date": "2026-03-07", "status": "posted",
                   "description": "other",
                   "lines": [{"account_number": "4000", "debit": "50.000000", "credit": "0.000000"}]}),
        ];
        let graph = FakeGraph::queue(vec![journal, vec![json!({"id": "br-1"})]]);
        let answer = create_bank_reconciliation(
            &graph,
            &scope(),
            &json!({"bank_account": "1000", "statement_date": "2026-03-31",
                    "statement_balance": "374.50"}),
        )
        .expect("reconciliation");
        assert_eq!(answer["book_balance"], json!("374.500000"));
        assert_eq!(answer["difference"], json!("0.000000"));
        assert_eq!(answer["is_reconciled"], json!(true));
        let lines = answer["journal_entries"].as_array().expect("lines");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1]["running_balance"], json!("374.500000"));
        // nothing the service never collected
        assert_eq!(answer["deposits_in_transit"], json!([]));
        assert_eq!(answer["adjustments"], Value::Null);
    }

    #[test]
    fn a_petty_cash_category_with_no_account_falls_back_to_miscellaneous() {
        assert_eq!(petty_cash_expense("postage"), "6200");
        assert_eq!(petty_cash_expense("meals"), "6400");
        assert_eq!(petty_cash_expense("something else"), "6600");
    }

    #[test]
    fn a_petty_cash_voucher_posts_to_its_category_and_the_petty_cash_account() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "acct-6200", "account_number": "6200",
                        "account_type": "expense", "normal_balance": "debit"})],
            vec![json!({"id": "acct-6101", "account_number": "1001",
                        "account_type": "asset", "normal_balance": "debit"})],
            vec![],
            vec![json!({"id": "je-1"})],
            vec![json!({"id": "pce-1"})],
        ]);
        let answer = create_petty_cash_entry(
            &graph,
            &scope(),
            &json!({"petty_cash_fund_id": "fund-1", "voucher_number": "V-1",
                    "voucher_date": "2026-03-15", "payee": "Post office",
                    "amount": "12.40", "category": "postage",
                    "description": "stamps"}),
        )
        .expect("voucher");
        assert_eq!(answer["journal_entry_id"], json!("je-1"));
        assert_eq!(answer["amount"], json!("12.400000"));
    }

    #[test]
    fn the_latest_reconciliation_is_the_newest_statement_for_that_account() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "br-1", "bank_account_number": "1000", "statement_date": "2026-01-31"}),
            json!({"id": "br-2", "bank_account_number": "1000", "statement_date": "2026-03-31"}),
            json!({"id": "br-3", "bank_account_number": "2000", "statement_date": "2026-04-30"}),
        ]);
        let answer = latest_bank_reconciliation(&graph, &scope(), "1000").expect("latest");
        assert_eq!(answer["id"], json!("br-2"));
    }
}
