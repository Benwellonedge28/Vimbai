#![forbid(unsafe_code)]
//! company-accounting-service, on the store.
//!
//! Companies and their equity record: the shareholder register, share capital, capital transactions,
//! dividends and their per-shareholder payments, retained earnings, and reserves. Three reports are
//! derived from those records: the statement of changes in equity, the shareholder register, and the
//! dividend history.
//!
//! Every amount is exact. Share capital, capital transactions, dividends, payments, retained earnings
//! and reserves all add, subtract and multiply in decimals, so a capital figure past double size keeps
//! its digits. Shareholdings are percentages, which are ratios rather than money, and are computed as
//! floats exactly as the service did.
//!
//! The rules, as shipped:
//!
//! * the register is one register: this service and issued-share-capital-service write the same
//!   shareholder rows, so a shareholder registered once appears to both,
//! * registering a shareholder recalculates every holding in that company as a share of the total, and
//!   leaves them untouched when the total is zero,
//! * share capital's paid-up total is issued shares times paid-up value per share; premium is carried
//!   separately and is not added to it,
//! * dividend payments are generated only for shareholders whose class matches the dividend's, and the
//!   withholding on those payments is a flat ten percent of the gross, whatever withholding the dividend
//!   itself declared,
//! * retained earnings close at opening plus profit, less dividends and transfers to reserves, plus
//!   prior-year adjustments, and reserves close at opening plus transfers in, less transfers out,
//! * the equity statement takes the newest share capital and retained-earnings records.
//!
//! What this does not do is post: creating a company, recording a capital transaction, declaring a
//! dividend and paying one each built a journal entry and asked another service to write it. Those
//! entries are returned with the record instead, so the caller decides who posts them. The four equity
//! accounts a new company needs are returned the same way.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount, wire};
use vimbai_store::{
    created_record, BookScope, Graph, Records, StoreError, CAPITAL_TRANSACTION, COMPANY,
    COMPANY_RESERVE, COMPANY_SHARE_CAPITAL, DIVIDEND, DIVIDEND_PAYMENT, RETAINED_EARNINGS,
    SHAREHOLDER,
};

/// What a create answers: the record as this service wrote it, carrying the id the store gave it.
///
/// The driver echoes the stored node, but answering from the body means the caller always sees the
/// fields this service computed, whatever shape the store answered with.
fn written(body: Value, stored: &Value) -> Value {
    let mut answer = created_record(stored);
    match (answer.as_object_mut(), body.as_object()) {
        (Some(answer), Some(fields)) => {
            for (key, value) in fields {
                answer.entry(key.clone()).or_insert_with(|| value.clone());
            }
            Value::Object(answer.clone())
        }
        _ => body,
    }
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

fn count(payload: &Value, key: &str) -> i64 {
    payload.get(key).and_then(Value::as_i64).unwrap_or_default()
}

fn field<'a>(record: &'a Value, key: &str) -> &'a str {
    record.get(key).and_then(Value::as_str).unwrap_or("")
}

fn shares(record: &Value) -> i64 {
    record
        .get("shares_held")
        .and_then(Value::as_i64)
        .unwrap_or_default()
}

fn amount_or_zero(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(Money::zero()),
        _ => amount(payload, key),
    }
}

fn kept<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    kind: vimbai_store::RecordKind,
    company_id: &str,
) -> Result<Vec<Value>, StoreError> {
    Ok(Records::new(graph, kind)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| field(row, "company_id") == company_id)
        .collect())
}

/// The newest of a set of records: listing is newest first, so the first row is the latest.
fn newest(rows: &[Value]) -> Option<&Value> {
    rows.first()
}

// --- companies ---

/// `/companies`: the company, plus the four equity accounts it needs.
pub fn create_company<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "company_name");
    let code = text(payload, "company_code");
    let company_body = json!({
        "company_code": code,
        "company_name": name,
        "registration_number": text(payload, "registration_number"),
        "company_type": or_default(payload, "company_type", "private_limited"),
        "incorporation_date": text(payload, "incorporation_date"),
        "financial_year_end": text(payload, "financial_year_end"),
        "registered_office": text(payload, "registered_office"),
        "jurisdiction": text(payload, "jurisdiction"),
        "tax_id": text(payload, "tax_id"),
        "status": or_default(payload, "status", "active"),
        "accounting_standard": or_default(payload, "accounting_standard", "IFRS"),
        "functional_currency": or_default(payload, "functional_currency", "USD"),
        "parent_company_id": text(payload, "parent_company_id"),
        "created_at": text(payload, "created_at"),
        "updated_at": text(payload, "created_at"),
    });
    let company = written(
        company_body.clone(),
        &Records::new(graph, COMPANY).create(scope, &company_body)?,
    );

    let equity_accounts: Vec<Value> = [
        ("SHARE_CAPITAL", "Share Capital"),
        ("SHARE_PREMIUM", "Share Premium Account"),
        ("RETAINED_EARNINGS", "Retained Earnings"),
        ("PROFIT_LOSS_CURRENT", "Profit & Loss - Current Year"),
    ]
    .iter()
    .map(|(suffix, label)| {
        json!({
            "account_number": format!("{code}-{suffix}"),
            "account_name": format!("{label} - {name}"),
            "account_type": "Equity",
            "description": format!("Equity account for {name}"),
        })
    })
    .collect();

    Ok(json!({"company": company, "equity_accounts": equity_accounts}))
}

/// `/companies` listing.
pub fn list_companies<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let kind = optional(query, "company_type");
    let status = optional(query, "status");
    let rows = Records::new(graph, COMPANY).list(scope, 1000)?;
    let kept: Vec<Value> = rows
        .into_iter()
        .filter(|row| {
            kind.as_deref()
                .map(|want| field(row, "company_type") == want)
                .unwrap_or(true)
                && status
                    .as_deref()
                    .map(|want| field(row, "status") == want)
                    .unwrap_or(true)
        })
        .collect();
    Ok(Value::Array(kept))
}

/// `/companies/{id}`.
pub fn get_company<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, COMPANY).get(scope, company_id)
}

/// `/companies/{id}` update.
pub fn update_company<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, COMPANY);
    let existing = records.get(scope, company_id)?;
    let keep = |key: &str| -> Value {
        match optional(payload, key) {
            Some(value) => json!(value),
            None => json!(field(&existing, key)),
        }
    };
    records.update(
        scope,
        company_id,
        &json!({
            "company_code": keep("company_code"),
            "company_name": keep("company_name"),
            "registration_number": text(payload, "registration_number"),
            "company_type": or_default(payload, "company_type", field(&existing, "company_type")),
            "incorporation_date": keep("incorporation_date"),
            "financial_year_end": keep("financial_year_end"),
            "registered_office": text(payload, "registered_office"),
            "jurisdiction": keep("jurisdiction"),
            "tax_id": text(payload, "tax_id"),
            "status": or_default(payload, "status", field(&existing, "status")),
            "accounting_standard": or_default(
                payload,
                "accounting_standard",
                field(&existing, "accounting_standard"),
            ),
            "functional_currency": or_default(
                payload,
                "functional_currency",
                field(&existing, "functional_currency"),
            ),
            "parent_company_id": text(payload, "parent_company_id"),
            "updated_at": text(payload, "updated_at"),
        }),
    )
}

// --- register ---

/// `/shareholders`: registers the holder and recalculates every holding in the company.
pub fn register_shareholder<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let register = Records::new(graph, SHAREHOLDER);
    let written =
        created_record(&register.create(scope, &vimbai_store::shareholder_record(payload))?);
    let company_id = field(&written, "company_id").to_string();

    let holders: Vec<Value> = Records::new(graph, SHAREHOLDER)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| field(row, "company_id") == company_id)
        .collect();
    let total: i64 = holders.iter().map(shares).sum();
    if total > 0 {
        for holder in &holders {
            let percentage = (shares(holder) as f64 / total as f64) * 100.0;
            let id = field(holder, "id").to_string();
            register.update(scope, &id, &json!({"percentage_holding": percentage}))?;
        }
    }

    register.get(scope, field(&written, "id"))
}

/// `/shareholders` listing.
pub fn list_shareholders<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let company = optional(query, "company_id");
    let class = optional(query, "share_class");
    let rows: Vec<Value> = Records::new(graph, SHAREHOLDER)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| {
            company
                .as_deref()
                .map(|want| field(row, "company_id") == want)
                .unwrap_or(true)
                && class
                    .as_deref()
                    .map(|want| field(row, "share_class") == want)
                    .unwrap_or(true)
        })
        .collect();
    Ok(Value::Array(rows))
}

// --- capital ---

/// `/share-capital`: paid-up capital is issued shares times paid-up value per share.
pub fn create_share_capital<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let per_share = amount(payload, "paid_up_value_per_share")?;
    let issued = count(payload, "issued_shares");
    let paid_up = per_share.times(&Money::from_i64(issued));
    let body = json!({
        "company_id": text(payload, "company_id"),
        "share_class": or_default(payload, "share_class", "ordinary"),
        "authorized_shares": count(payload, "authorized_shares"),
        "issued_shares": issued,
        "paid_up_value_per_share": wire(&per_share),
        "total_paid_up_capital": wire(&paid_up),
        "share_premium": wire(&amount_or_zero(payload, "share_premium")?),
        "par_value": wire(&amount_or_zero(payload, "par_value")?),
        "currency": or_default(payload, "currency", "USD"),
        "as_of_date": text(payload, "as_of_date"),
        "created_at": text(payload, "created_at"),
    });
    Ok(written(
        body.clone(),
        &Records::new(graph, COMPANY_SHARE_CAPITAL).create(scope, &body)?,
    ))
}

/// `/share-capital` listing.
pub fn list_share_capital<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    match optional(query, "company_id") {
        Some(company_id) => Ok(Value::Array(kept(
            graph,
            scope,
            COMPANY_SHARE_CAPITAL,
            &company_id,
        )?)),
        None => Ok(Value::Array(
            Records::new(graph, COMPANY_SHARE_CAPITAL).list(scope, 1000)?,
        )),
    }
}

fn journal(date: &str, description: &str, reference: &str, lines: Value) -> Value {
    json!({
        "description": description,
        "reference": reference,
        "date": date,
        "lines": lines,
    })
}

/// `/capital-transactions`: the transaction, plus the journal entry it implies.
pub fn record_capital_transaction<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let kind = or_default(payload, "transaction_type", "share_issuance");
    let total = amount(payload, "total_amount")?;
    let price = amount(payload, "price_per_share")?;
    let number = count(payload, "number_of_shares");
    let premium = amount_or_zero(payload, "share_premium_amount")?;
    let date = text(payload, "transaction_date");
    let transaction_body = json!({
        "company_id": text(payload, "company_id"),
        "transaction_type": kind,
        "transaction_date": date,
        "share_class": text(payload, "share_class"),
        "number_of_shares": number,
        "price_per_share": wire(&price),
        "total_amount": wire(&total),
        "share_premium_amount": wire(&premium),
        "reason": text(payload, "reason"),
        "shareholder_id": text(payload, "shareholder_id"),
        "reference_number": text(payload, "reference_number"),
        "approved_by": text(payload, "approved_by"),
        "journal_entry": Value::Null,
        "notes": text(payload, "notes"),
        "created_at": text(payload, "created_at"),
    });
    let transaction = written(
        transaction_body.clone(),
        &Records::new(graph, CAPITAL_TRANSACTION).create(scope, &transaction_body)?,
    );

    let company = optional(payload, "company_id")
        .map(|id| Records::new(graph, COMPANY).get(scope, &id))
        .transpose()?
        .unwrap_or(Value::Null);
    if company.is_null() {
        return Ok(json!({"transaction": transaction, "journal_entry": Value::Null}));
    }
    let code = field(&company, "company_code").to_string();

    let lines = match kind.as_str() {
        "share_issuance" => {
            let nominal = price.times(&Money::from_i64(number));
            let mut lines = vec![
                json!({"account_code": format!("{code}-BANK"),
                       "description": format!("Share issuance - {number} shares"),
                       "debit": true, "amount": wire(&total)}),
                json!({"account_code": format!("{code}-SHARE_CAPITAL"),
                       "description": "Share capital - nominal value",
                       "debit": false, "amount": wire(&nominal)}),
            ];
            if !premium.is_zero() {
                lines.push(json!({"account_code": format!("{code}-SHARE_PREMIUM"),
                                  "description": "Share premium",
                                  "debit": false, "amount": wire(&premium)}));
            }
            lines
        }
        "dividend_payment" => vec![
            json!({"account_code": format!("{code}-DIVIDEND"),
                   "description": "Dividend payment", "debit": true, "amount": wire(&total)}),
            json!({"account_code": format!("{code}-BANK"),
                   "description": "Cash paid for dividends", "debit": false,
                   "amount": wire(&total)}),
        ],
        _ => Vec::new(),
    };

    if lines.is_empty() {
        return Ok(json!({"transaction": transaction, "journal_entry": Value::Null}));
    }
    let description =
        optional(payload, "reason").unwrap_or_else(|| format!("Capital transaction: {kind}"));
    Ok(json!({
        "transaction": transaction,
        "journal_entry": journal(&date, &description, &text(payload, "reference_number"), json!(lines)),
    }))
}

/// `/capital-transactions` listing, newest first.
pub fn list_capital_transactions<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let company = optional(query, "company_id");
    let kind = optional(query, "transaction_type");
    let start = optional(query, "start_date");
    let end = optional(query, "end_date");
    let mut rows: Vec<Value> = Records::new(graph, CAPITAL_TRANSACTION)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| {
            company
                .as_deref()
                .map(|want| field(row, "company_id") == want)
                .unwrap_or(true)
                && kind
                    .as_deref()
                    .map(|want| field(row, "transaction_type") == want)
                    .unwrap_or(true)
                && start
                    .as_deref()
                    .map(|want| field(row, "transaction_date") >= want)
                    .unwrap_or(true)
                && end
                    .as_deref()
                    .map(|want| field(row, "transaction_date") <= want)
                    .unwrap_or(true)
        })
        .collect();
    rows.sort_by(|left, right| {
        field(right, "transaction_date").cmp(field(left, "transaction_date"))
    });
    Ok(Value::Array(rows))
}

// --- dividends ---

/// `/dividends`: declares it, writes one payment per matching holder, and returns the entry.
pub fn declare_dividend<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let total = amount(payload, "total_amount")?;
    let withheld = amount_or_zero(payload, "tax_withheld")?;
    let per_share = amount(payload, "per_share_amount")?;
    let kind = or_default(payload, "dividend_type", "final");
    let class = or_default(payload, "share_class", "ordinary");
    let company_id = text(payload, "company_id");

    let dividend_body = json!({
        "company_id": company_id,
        "dividend_type": kind,
        "declaration_date": text(payload, "declaration_date"),
        "share_class": class,
        "total_amount": wire(&total),
        "tax_withheld": wire(&withheld),
        "net_payment": wire(&total.minus(&withheld)),
        "per_share_amount": wire(&per_share),
        "status": or_default(payload, "status", "declared"),
        "approved_by": text(payload, "approved_by"),
        "created_at": text(payload, "created_at"),
    });
    let dividend = written(
        dividend_body.clone(),
        &Records::new(graph, DIVIDEND).create(scope, &dividend_body)?,
    );
    let dividend_id = field(&dividend, "id").to_string();

    // ten percent withheld on each holder's payment, whatever the dividend itself declared
    let rate = Money::from_i64(10);
    let hundred = Money::from_i64(100);
    let payments = Records::new(graph, DIVIDEND_PAYMENT);
    let mut answered = Vec::new();
    for holder in kept(graph, scope, SHAREHOLDER, &company_id)? {
        if field(&holder, "share_class") != class {
            continue;
        }
        let held = Money::from_i64(shares(&holder));
        let gross = held.times(&per_share);
        let tax = gross.times(&rate).over(&hundred);
        let payment = json!({
            "dividend_id": dividend_id,
            "shareholder_id": field(&holder, "id"),
            "shareholder_name": field(&holder, "shareholder_name"),
            "shares_held": shares(&holder),
            "gross_amount": wire(&gross),
            "tax_withheld": wire(&tax),
            "net_amount": wire(&gross.minus(&tax)),
            "status": "pending",
            "payment_date": Value::Null,
        });
        answered.push(written(payment.clone(), &payments.create(scope, &payment)?));
    }

    let company = Records::new(graph, COMPANY).get(scope, &company_id).ok();
    let entry = company.map(|company| {
        let code = field(&company, "company_code").to_string();
        journal(
            &text(payload, "declaration_date"),
            &format!("Dividend declared - {kind}"),
            &format!("DIV-{}", &dividend_id[..dividend_id.len().min(8)]),
            json!([
                json!({"account_code": format!("{code}-PROFIT_LOSS_CURRENT"),
                       "description": "Proposed dividend", "debit": true, "amount": wire(&total)}),
                json!({"account_code": format!("{code}-DIVIDEND_PAYABLE"),
                       "description": "Dividend payable", "debit": false, "amount": wire(&total)}),
            ]),
        )
    });

    Ok(json!({
        "dividend": dividend,
        "payments": answered,
        "journal_entry": entry.unwrap_or(Value::Null),
    }))
}

/// `/dividends` listing.
pub fn list_dividends<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let company = optional(query, "company_id");
    let status = optional(query, "status");
    let rows: Vec<Value> = Records::new(graph, DIVIDEND)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| {
            company
                .as_deref()
                .map(|want| field(row, "company_id") == want)
                .unwrap_or(true)
                && status
                    .as_deref()
                    .map(|want| field(row, "status") == want)
                    .unwrap_or(true)
        })
        .collect();
    Ok(Value::Array(rows))
}

/// `/dividends/{id}/pay`: marks the dividend and its payments paid, and returns the entry.
pub fn pay_dividend<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    dividend_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let dividends = Records::new(graph, DIVIDEND);
    let existing = dividends.get(scope, dividend_id)?;
    let approved_by = text(payload, "approved_by");
    let paid_on = text(payload, "payment_date");
    let dividend = created_record(&dividends.update(
        scope,
        dividend_id,
        &json!({"status": "paid", "approved_by": approved_by}),
    )?);

    let payments = Records::new(graph, DIVIDEND_PAYMENT);
    for payment in Records::new(graph, DIVIDEND_PAYMENT).list(scope, 1000)? {
        if field(&payment, "dividend_id") != dividend_id {
            continue;
        }
        let id = field(&payment, "id").to_string();
        payments.update(
            scope,
            &id,
            &json!({"status": "processed", "payment_date": paid_on}),
        )?;
    }

    let company = Records::new(graph, COMPANY)
        .get(scope, field(&existing, "company_id"))
        .ok();
    let entry = company.map(|company| {
        let code = field(&company, "company_code").to_string();
        journal(
            &paid_on,
            &format!("Dividend payment - {}", field(&existing, "dividend_type")),
            &format!("DIV-PAY-{}", &dividend_id[..dividend_id.len().min(8)]),
            json!([
                json!({"account_code": format!("{code}-DIVIDEND_PAYABLE"),
                       "description": "Clear dividend payable", "debit": true,
                       "amount": field(&existing, "total_amount")}),
                json!({"account_code": format!("{code}-BANK"),
                       "description": "Cash paid for dividends", "debit": false,
                       "amount": field(&existing, "net_payment")}),
                json!({"account_code": format!("{code}-TAX"),
                       "description": "Tax withheld on dividends", "debit": false,
                       "amount": field(&existing, "tax_withheld")}),
            ]),
        )
    });

    Ok(json!({"dividend": dividend, "journal_entry": entry.unwrap_or(Value::Null)}))
}

/// `/dividends/{id}/payments`.
pub fn dividend_payments<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    dividend_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, DIVIDEND).get(scope, dividend_id)?;
    let rows: Vec<Value> = Records::new(graph, DIVIDEND_PAYMENT)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| field(row, "dividend_id") == dividend_id)
        .collect();
    Ok(Value::Array(rows))
}

// --- retained earnings and reserves ---

/// `/retained-earnings`: closing balance from the movement it carries.
pub fn record_retained_earnings<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let opening = amount(payload, "opening_balance")?;
    let profit = amount_or_zero(payload, "net_profit_for_period")?;
    let dividends = amount_or_zero(payload, "dividends_declared")?;
    let prior = amount_or_zero(payload, "prior_year_adjustments")?;
    let transfers = amount_or_zero(payload, "transfers_to_reserves")?;
    let closing = opening
        .plus(&profit)
        .minus(&dividends)
        .plus(&prior)
        .minus(&transfers);
    let body = json!({
        "company_id": text(payload, "company_id"),
        "period": text(payload, "period"),
        "opening_balance": wire(&opening),
        "net_profit_for_period": wire(&profit),
        "dividends_declared": wire(&dividends),
        "prior_year_adjustments": wire(&prior),
        "transfers_to_reserves": wire(&transfers),
        "closing_balance": wire(&closing),
        "created_at": text(payload, "created_at"),
    });
    Ok(written(
        body.clone(),
        &Records::new(graph, RETAINED_EARNINGS).create(scope, &body)?,
    ))
}

/// `/retained-earnings`, oldest first.
pub fn retained_earnings<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let mut rows = kept(graph, scope, RETAINED_EARNINGS, company_id)?;
    rows.reverse();
    Ok(Value::Array(rows))
}

/// `/reserves`: closing balance from the transfers it carries.
pub fn create_reserve<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let opening = amount(payload, "opening_balance")?;
    let inward = amount_or_zero(payload, "transfers_in")?;
    let outward = amount_or_zero(payload, "transfers_out")?;
    let body = json!({
        "company_id": text(payload, "company_id"),
        "reserve_name": text(payload, "reserve_name"),
        "reserve_type": or_default(payload, "reserve_type", "general"),
        "opening_balance": wire(&opening),
        "transfers_in": wire(&inward),
        "transfers_out": wire(&outward),
        "closing_balance": wire(&opening.plus(&inward).minus(&outward)),
        "created_at": text(payload, "created_at"),
    });
    Ok(written(
        body.clone(),
        &Records::new(graph, COMPANY_RESERVE).create(scope, &body)?,
    ))
}

/// `/reserves`.
pub fn reserves<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    Ok(Value::Array(kept(
        graph,
        scope,
        COMPANY_RESERVE,
        company_id,
    )?))
}

// --- reports ---

/// `/reports/equity-statement/{company_id}`.
pub fn equity_statement<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let company = Records::new(graph, COMPANY).get(scope, company_id)?;
    let as_of = text(query, "as_of_date");

    let capitals = kept(graph, scope, COMPANY_SHARE_CAPITAL, company_id)?;
    let capital = newest(&capitals)
        .map(|row| -> Result<(Money, Money), StoreError> {
            Ok((
                vimbai_store::amounts::stored_amount(row, "total_paid_up_capital")?,
                vimbai_store::amounts::stored_amount(row, "share_premium")?,
            ))
        })
        .transpose()?;
    let (share_capital, share_premium) = capital.unwrap_or((Money::zero(), Money::zero()));

    let earnings = kept(graph, scope, RETAINED_EARNINGS, company_id)?;
    let retained = newest(&earnings)
        .map(|row| vimbai_store::amounts::stored_amount(row, "closing_balance"))
        .transpose()?
        .unwrap_or_else(Money::zero);

    let mut total_reserves = Money::zero();
    for reserve in kept(graph, scope, COMPANY_RESERVE, company_id)? {
        total_reserves = total_reserves.plus(&vimbai_store::amounts::stored_amount(
            &reserve,
            "closing_balance",
        )?);
    }

    let mut movements = Vec::new();
    for transaction in kept(graph, scope, CAPITAL_TRANSACTION, company_id)? {
        let date = field(&transaction, "transaction_date");
        if !as_of.is_empty() && date > as_of.as_str() {
            continue;
        }
        movements.push(json!({
            "date": date,
            "type": field(&transaction, "transaction_type"),
            "description": field(&transaction, "reason"),
            "amount": field(&transaction, "total_amount"),
        }));
    }

    let total_equity = share_capital
        .plus(&share_premium)
        .plus(&total_reserves)
        .plus(&retained);

    Ok(json!({
        "company_id": company_id,
        "report_date": as_of,
        "share_capital": wire(&share_capital),
        "share_premium": wire(&share_premium),
        "reserves": wire(&total_reserves),
        "retained_earnings": wire(&retained),
        "total_equity": wire(&total_equity),
        "movements": movements,
        "company": company,
    }))
}

/// `/reports/shareholder-register/{company_id}`.
pub fn shareholder_register<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let company = Records::new(graph, COMPANY).get(scope, company_id)?;
    let mut holders = kept(graph, scope, SHAREHOLDER, company_id)?;
    holders.sort_by(|left, right| {
        let left = left
            .get("percentage_holding")
            .and_then(Value::as_f64)
            .unwrap_or_default();
        let right = right
            .get("percentage_holding")
            .and_then(Value::as_f64)
            .unwrap_or_default();
        right
            .partial_cmp(&left)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let rows: Vec<Value> = holders
        .iter()
        .map(|holder| {
            let percentage = holder
                .get("percentage_holding")
                .and_then(Value::as_f64)
                .unwrap_or_default();
            json!({
                "name": field(holder, "shareholder_name"),
                "type": field(holder, "shareholder_type"),
                "shares_held": shares(holder),
                // two decimal places and a percent sign, as the report always printed it
                "percentage": format!("{percentage:.2}%"),
                "controlling_party": holder
                    .get("is_controlling_party")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect();

    Ok(json!({
        "company": company,
        "total_shareholders": rows.len(),
        "shareholders": rows,
    }))
}

/// `/reports/dividend-history/{company_id}`.
pub fn dividend_history<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let mut rows = kept(graph, scope, DIVIDEND, company_id)?;
    rows.sort_by(|left, right| {
        field(right, "declaration_date").cmp(field(left, "declaration_date"))
    });
    let mut total = Money::zero();
    for row in &rows {
        total = total.plus(&vimbai_store::amounts::stored_amount(row, "total_amount")?);
    }
    Ok(json!({
        "company_id": company_id,
        "total_dividends_declared": rows.len(),
        "total_amount": wire(&total),
        "dividends": rows,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn holder(id: &str, class: &str, held: i64) -> Value {
        json!({"id": id, "company_id": "c-1", "shareholder_name": id, "share_class": class,
               "shares_held": held, "percentage_holding": 0.0, "shareholder_type": "individual"})
    }

    #[test]
    fn a_company_carries_the_four_equity_accounts_it_needs() {
        let graph = FakeGraph::returning(vec![json!({"id": "c-1"})]);
        let answer = create_company(
            &graph,
            &scope(),
            &json!({"company_code": "ACME", "company_name": "Acme Ltd",
                    "incorporation_date": "2020-01-01T00:00:00Z"}),
        )
        .expect("created");
        let accounts = answer["equity_accounts"].as_array().expect("accounts");
        assert_eq!(accounts.len(), 4);
        assert_eq!(accounts[0]["account_number"], json!("ACME-SHARE_CAPITAL"));
        assert_eq!(
            accounts[0]["account_name"],
            json!("Share Capital - Acme Ltd")
        );
        // and the company itself was stored
        assert_eq!(answer["company"]["id"], json!("c-1"));
    }

    #[test]
    fn registering_a_holder_recalculates_every_holding() {
        let graph = FakeGraph::queue(vec![
            vec![holder("h-1", "ordinary", 250)],
            vec![
                holder("h-1", "ordinary", 250),
                holder("h-2", "ordinary", 750),
            ],
            vec![json!({"id": "h-1"})],
            vec![json!({"id": "h-2"})],
            vec![holder("h-1", "ordinary", 250)],
        ]);
        let answer = register_shareholder(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "shareholder_name": "Ana", "shares_held": 250}),
        )
        .expect("registered");
        assert_eq!(answer["id"], json!("h-1"));
        let (_, first) = graph.statement(2);
        assert_eq!(first["patch"]["percentage_holding"], json!(25.0));
        let (_, second) = graph.statement(3);
        assert_eq!(second["patch"]["percentage_holding"], json!(75.0));
    }

    #[test]
    fn a_company_with_no_shares_leaves_the_percentages_alone() {
        let graph = FakeGraph::queue(vec![
            vec![holder("h-1", "ordinary", 0)],
            vec![holder("h-1", "ordinary", 0)],
            vec![holder("h-1", "ordinary", 0)],
        ]);
        register_shareholder(&graph, &scope(), &json!({"company_id": "c-1"})).expect("registered");
        // create, list, and the re-read: no update statement was issued
        let (cypher, _) = graph.statement(1);
        assert!(cypher.contains("RETURN x"), "{cypher}");
        assert!(!cypher.contains("SET x +="), "{cypher}");
    }

    #[test]
    fn paid_up_capital_is_issued_shares_times_the_paid_up_value() {
        let graph = FakeGraph::returning(vec![json!({"id": "sc-1"})]);
        create_share_capital(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "share_class": "ordinary", "authorized_shares": 10000,
                    "issued_shares": 1000, "paid_up_value_per_share": "123456789012345678.90155",
                    "share_premium": "0"}),
        )
        .expect("created");
        let (_, params) = graph.last();
        assert_eq!(
            params["props"]["total_paid_up_capital"],
            json!("123456789012345678901.550000")
        );
    }

    #[test]
    fn a_share_issuance_returns_the_entry_it_would_post() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "t-1"})],
            vec![json!({"id": "c-1", "company_code": "ACME"})],
        ]);
        let answer = record_capital_transaction(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "transaction_type": "share_issuance",
                    "transaction_date": "2026-03-01T00:00:00Z", "number_of_shares": 100,
                    "price_per_share": "10", "total_amount": "1100",
                    "share_premium_amount": "100", "reference_number": "R-1"}),
        )
        .expect("recorded");
        let lines = answer["journal_entry"]["lines"].as_array().expect("lines");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["account_code"], json!("ACME-BANK"));
        assert_eq!(lines[0]["amount"], json!("1100.000000"));
        // nominal is shares times price, not the total, so the premium stays visible
        assert_eq!(lines[1]["amount"], json!("1000.000000"));
        assert_eq!(lines[2]["amount"], json!("100.000000"));
    }

    #[test]
    fn a_capital_transaction_of_another_kind_posts_nothing() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "t-1"})],
            vec![json!({"id": "c-1", "company_code": "ACME"})],
        ]);
        let answer = record_capital_transaction(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "transaction_type": "share_transfer",
                    "transaction_date": "2026-03-01T00:00:00Z", "price_per_share": "10",
                    "total_amount": "10"}),
        )
        .expect("recorded");
        assert_eq!(answer["journal_entry"], Value::Null);
    }

    #[test]
    fn a_dividend_pays_only_the_matching_class_and_withholds_ten_percent() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "d-1"})],
            vec![
                holder("h-1", "ordinary", 100),
                holder("h-2", "preference", 100),
            ],
            vec![json!({"id": "p-1"})],
            vec![json!({"id": "c-1", "company_code": "ACME"})],
        ]);
        let answer = declare_dividend(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "dividend_type": "final", "share_class": "ordinary",
                    "declaration_date": "2026-03-01T00:00:00Z", "total_amount": "50",
                    "tax_withheld": "5", "per_share_amount": "0.50"}),
        )
        .expect("declared");
        let payments = answer["payments"].as_array().expect("payments");
        assert_eq!(payments.len(), 1);
        assert_eq!(payments[0]["gross_amount"], json!("50.000000"));
        assert_eq!(payments[0]["tax_withheld"], json!("5.000000"));
        assert_eq!(payments[0]["net_amount"], json!("45.000000"));
        assert_eq!(answer["dividend"]["net_payment"], json!("45.000000"));
    }

    #[test]
    fn retained_earnings_close_on_the_movement_they_carry() {
        let graph = FakeGraph::returning(vec![json!({"id": "re-1"})]);
        record_retained_earnings(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "opening_balance": "1000",
                    "net_profit_for_period": "250.25", "dividends_declared": "100",
                    "prior_year_adjustments": "10", "transfers_to_reserves": "60"}),
        )
        .expect("recorded");
        let (_, params) = graph.last();
        // 1000 + 250.25 - 100 + 10 - 60
        assert_eq!(params["props"]["closing_balance"], json!("1100.250000"));
    }

    #[test]
    fn the_equity_statement_adds_the_newest_records_up() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "c-1", "company_code": "ACME"})],
            vec![
                json!({"id": "sc-1", "company_id": "c-1", "total_paid_up_capital": "1000",
                        "share_premium": "200"}),
            ],
            vec![json!({"id": "re-1", "company_id": "c-1", "closing_balance": "300.55"})],
            vec![
                json!({"id": "r-1", "company_id": "c-1", "closing_balance": "50"}),
                json!({"id": "r-2", "company_id": "c-1", "closing_balance": "25"}),
            ],
            vec![
                json!({"id": "t-1", "company_id": "c-1", "transaction_type": "share_issuance",
                        "transaction_date": "2026-03-01T00:00:00Z",
                        "total_amount": "1200", "reason": "Issue"}),
            ],
        ]);
        let answer = equity_statement(
            &graph,
            &scope(),
            "c-1",
            &json!({"as_of_date": "2026-03-31T00:00:00Z"}),
        )
        .expect("statement");
        assert_eq!(answer["share_capital"], json!("1000.000000"));
        assert_eq!(answer["reserves"], json!("75.000000"));
        assert_eq!(answer["retained_earnings"], json!("300.550000"));
        assert_eq!(answer["total_equity"], json!("1575.550000"));
        assert_eq!(answer["movements"].as_array().expect("movements").len(), 1);
    }

    #[test]
    fn a_movement_after_the_report_date_is_left_out() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "c-1", "company_code": "ACME"})],
            vec![],
            vec![],
            vec![],
            vec![
                json!({"id": "t-1", "company_id": "c-1", "transaction_type": "share_issuance",
                        "transaction_date": "2026-05-01T00:00:00Z", "total_amount": "1"}),
            ],
        ]);
        let answer = equity_statement(
            &graph,
            &scope(),
            "c-1",
            &json!({"as_of_date": "2026-03-31T00:00:00Z"}),
        )
        .expect("statement");
        assert!(answer["movements"]
            .as_array()
            .expect("movements")
            .is_empty());
        assert_eq!(answer["total_equity"], json!("0.000000"));
    }

    #[test]
    fn the_register_prints_percentages_to_two_places() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "c-1", "company_code": "ACME"})],
            vec![
                json!({"id": "h-1", "company_id": "c-1", "shareholder_name": "Ana",
                       "shareholder_type": "individual", "shares_held": 999,
                       "percentage_holding": 33.3333, "is_controlling_party": false}),
                json!({"id": "h-2", "company_id": "c-1", "shareholder_name": "Bem",
                       "shareholder_type": "corporate", "shares_held": 2001,
                       "percentage_holding": 66.6667, "is_controlling_party": true}),
            ],
        ]);
        let answer = shareholder_register(&graph, &scope(), "c-1").expect("register");
        let rows = answer["shareholders"].as_array().expect("rows");
        assert_eq!(rows[0]["name"], json!("Bem"));
        assert_eq!(rows[0]["percentage"], json!("66.67%"));
        assert_eq!(rows[1]["percentage"], json!("33.33%"));
    }
}
