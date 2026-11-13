#![forbid(unsafe_code)]
//! ordinary-shares-service and preference-shares-service, on the store.
//!
//! Both answer the same two questions - what dividend has been declared, and has it been paid -
//! and they differ in what a share class is worth. An ordinary dividend is simply so much per
//! share. A preference dividend is the same, plus whatever was missed in earlier periods, because
//! that arrears figure is the whole point of preference stock.
//!
//! Preference classes also carry their own capital arithmetic: issuing raises the issued and
//! outstanding counts and splits the proceeds into nominal and premium, and redeeming lowers the
//! outstanding count and splits the price into nominal and the capital redemption reserve.
//!
//! Both services used to send the journal entry they built to the accounting service over HTTP.
//! Here the entry is built and returned with the record, and nothing is posted: the ledger is the
//! accounting crate's to own, and a share transaction that both wrote the ledger and reported its
//! own figures could disagree with itself.
//!
//! Kept as shipped:
//!
//! * A declaration defaults to `declared`, a payment sets `paid` and stamps the payment date.
//! * Preference arrears are added to the declared total, and can be negative - the service never
//!   checked, and a credit against an earlier overpayment is a real thing.
//! * Redeeming more shares than are outstanding is not refused; the service subtracted anyway and
//!   so does this, with the negative outstanding count visible rather than hidden.
//! * Paying a dividend that does not exist is a 404 here. The service returned `{"error": ...}`
//!   with a 200, which reads as success to any client that checks the status.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, wire};
use vimbai_store::read::{text, whole_signed};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, ORDINARY_DIVIDEND, PREFERENCE_CLASS,
    PREFERENCE_DIVIDEND, PREFERENCE_REDEMPTION,
};

fn count_in(record: &Value, key: &str) -> i64 {
    record.get(key).and_then(Value::as_i64).unwrap_or(0)
}

/// A journal entry as both services built one: balanced by construction, and returned rather than
/// posted so the caller can hand it to the ledger once.
fn entry(date: &str, description: String, reference: String, lines: Vec<Value>) -> Value {
    json!({
        "date": date,
        "description": description,
        "entries": lines,
        "reference": reference,
    })
}

fn line(code: &str, name: &str, debit: &Money, credit: &Money) -> Value {
    json!({
        "account_code": code,
        "description": name,
        "debit": wire(debit),
        "credit": wire(credit),
    })
}

fn zero() -> Money {
    Money::zero()
}

// ---------- ordinary shares ----------

/// `/dividends/declare`: so much per share, over the shares in issue.
pub fn declare_ordinary_dividend<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let per_share = amount_or_zero(payload, "per_share_amount")?;
    let shares = whole_signed(payload, "total_shares")?;
    let total = per_share.times(&Money::from_i64(shares));
    let date = text(payload, "record_date");
    let kind = text(payload, "dividend_type");

    let journal = entry(
        &date,
        format!("Declaration of {kind} dividend"),
        format!("DIV-{}", &text(payload, "id")),
        vec![
            line("3300", "Retained Earnings", &total, &zero()),
            line("2310", "Dividend Payable", &zero(), &total),
        ],
    );

    Records::new(graph, ORDINARY_DIVIDEND).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "dividend_type": kind,
            "per_share_amount": wire(&per_share),
            "total_shares": shares,
            "total_dividend": wire(&total),
            "record_date": date,
            "status": "declared",
            "journal_entry": journal,
        }),
    )
}

/// `/dividends/{id}/pay`: mark it paid and keep the entry that released it.
pub fn pay_ordinary_dividend<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    dividend_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, ORDINARY_DIVIDEND);
    let dividend = records.get(scope, dividend_id)?;
    let total = vimbai_store::amounts::stored_amount(&dividend, "total_dividend")?;
    let paid_on = text(payload, "payment_date");

    let journal = entry(
        &paid_on,
        format!(
            "Payment of {} dividend",
            dividend
                .get("dividend_type")
                .and_then(Value::as_str)
                .unwrap_or("ordinary")
        ),
        format!("DIV-PAY-{dividend_id}"),
        vec![
            line("2310", "Dividend Payable", &total, &zero()),
            line("1000", "Bank", &zero(), &total),
        ],
    );

    records.update(
        scope,
        dividend_id,
        &json!({"status": "paid", "payment_date": paid_on, "payment_journal_entry": journal}),
    )
}

/// `/dividends`: the caller's dividends, optionally for one company.
pub fn list_ordinary_dividends<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, ORDINARY_DIVIDEND).list(scope, 1000)?;
    Ok(Value::Array(match company_id {
        Some(company) => rows
            .into_iter()
            .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company))
            .collect(),
        None => rows,
    }))
}

// ---------- preference shares ----------

/// `/classes/create`: a preference class and the terms that make it one.
pub fn create_preference_class<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.trim().is_empty() {
        return Err(StoreError::Invalid("name is required".into()));
    }

    Records::new(graph, PREFERENCE_CLASS).create(
        scope,
        &json!({
            "name": name,
            "company_id": text(payload, "company_id"),
            "nominal_value": wire(&amount_or_zero(payload, "nominal_value")?),
            "issue_price": wire(&amount_or_zero(payload, "issue_price")?),
            "fixed_dividend_rate": wire(&amount_or_zero(payload, "fixed_dividend_rate")?),
            "dividend_type": text(payload, "dividend_type"),
            "participation_rights": text(payload, "participation_rights"),
            "liquidation_priority": count_in(payload, "liquidation_priority"),
            "redemption_terms": payload.get("redemption_terms").cloned().unwrap_or(Value::Null),
            "conversion_terms": payload.get("conversion_terms").cloned().unwrap_or(Value::Null),
            "shares_issued": 0,
            "shares_outstanding": 0,
        }),
    )
}

pub fn list_preference_classes<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, PREFERENCE_CLASS).list(scope, 1000)?;
    let filtered: Vec<Value> = match company_id {
        Some(company) => rows
            .into_iter()
            .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company))
            .collect(),
        None => rows,
    };
    Ok(json!({ "share_classes": filtered }))
}

/// `/classes/{id}/issue`: proceeds split into nominal capital and premium.
pub fn issue_preference_shares<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    class_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, PREFERENCE_CLASS);
    let class = records.get(scope, class_id)?;
    let shares = whole_signed(payload, "shares_issued")?;
    let issued = count_in(&class, "shares_issued") + shares;
    let outstanding = count_in(&class, "shares_outstanding") + shares;

    let price = vimbai_store::amounts::stored_amount(&class, "issue_price")?;
    let nominal_value = vimbai_store::amounts::stored_amount(&class, "nominal_value")?;
    let proceeds = price.times(&Money::from_i64(shares));
    let nominal = nominal_value.times(&Money::from_i64(shares));
    let premium = proceeds.minus(&nominal);
    let date = text(payload, "issue_date");
    let name = class
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("preference");

    let journal = entry(
        &date,
        format!("Issue of {shares} {name} preference shares"),
        format!("PREF-ISS-{class_id}"),
        vec![
            line("1000", "Bank", &proceeds, &zero()),
            line("3205", "Preference Share Capital", &zero(), &nominal),
            line("3215", "Preference Share Premium", &zero(), &premium),
        ],
    );

    records.update(
        scope,
        class_id,
        &json!({
            "shares_issued": issued,
            "shares_outstanding": outstanding,
            "last_issue_journal_entry": journal,
        }),
    )
}

/// `/classes/{id}/dividends/declare`: the fixed rate plus what earlier periods missed.
pub fn declare_preference_dividend<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    class_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let class = Records::new(graph, PREFERENCE_CLASS).get(scope, class_id)?;
    let per_share = amount_or_zero(payload, "per_share_amount")?;
    let shares = whole_signed(payload, "total_shares")?;
    let arrears = amount_or_zero(payload, "preference_arears")?;
    let total = per_share.times(&Money::from_i64(shares)).plus(&arrears);
    let date = text(payload, "record_date");
    let name = class
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("preference");

    let journal = entry(
        &date,
        format!("Declaration of {name} preference dividend"),
        format!("PREF-DIV-{class_id}"),
        vec![
            line("3300", "Retained Earnings", &total, &zero()),
            line("2315", "Preference Dividend Payable", &zero(), &total),
        ],
    );

    Records::new(graph, PREFERENCE_DIVIDEND).create(
        scope,
        &json!({
            "share_class_id": class_id,
            "company_id": text(payload, "company_id"),
            "dividend_type": "fixed",
            "per_share_amount": wire(&per_share),
            "total_shares": shares,
            "preference_arears": wire(&arrears),
            "total_dividend": wire(&total),
            "record_date": date,
            "status": "declared",
            "journal_entry": journal,
        }),
    )
}

pub fn pay_preference_dividend<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    dividend_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, PREFERENCE_DIVIDEND);
    let dividend = records.get(scope, dividend_id)?;
    let total = vimbai_store::amounts::stored_amount(&dividend, "total_dividend")?;
    let paid_on = text(payload, "payment_date");

    let journal = entry(
        &paid_on,
        "Payment of preference dividend".to_string(),
        format!("PREF-DIV-PAY-{dividend_id}"),
        vec![
            line("2315", "Preference Dividend Payable", &total, &zero()),
            line("1000", "Bank", &zero(), &total),
        ],
    );

    records.update(
        scope,
        dividend_id,
        &json!({"status": "paid", "payment_date": paid_on, "payment_journal_entry": journal}),
    )
}

/// `/classes/{id}/redeem`: outstanding comes down, the excess over nominal goes to the reserve.
pub fn redeem_preference_shares<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    class_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, PREFERENCE_CLASS);
    let class = records.get(scope, class_id)?;
    let shares = whole_signed(payload, "shares_redeemed")?;
    let price = amount_or_zero(payload, "redemption_price")?;
    let total = price.times(&Money::from_i64(shares));
    let nominal_value = vimbai_store::amounts::stored_amount(&class, "nominal_value")?;
    let nominal = nominal_value.times(&Money::from_i64(shares));
    let reserve = total.minus(&nominal);
    let date = text(payload, "redemption_date");
    let name = class
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("preference");

    let journal = entry(
        &date,
        format!("Redemption of {shares} {name} preference shares"),
        format!("PREF-RED-{class_id}"),
        vec![
            line("3205", "Preference Share Capital", &nominal, &zero()),
            line("3220", "Capital Redemption Reserve", &reserve, &zero()),
            line("1000", "Bank", &zero(), &total),
        ],
    );

    let saved = Records::new(graph, PREFERENCE_REDEMPTION).create(
        scope,
        &json!({
            "share_class_id": class_id,
            "shares_redeemed": shares,
            "redemption_price": wire(&price),
            "total_proceeds": wire(&total),
            "redemption_date": date,
            "journal_entry": journal,
        }),
    )?;

    records.update(
        scope,
        class_id,
        &json!({
            "shares_outstanding": count_in(&class, "shares_outstanding") - shares,
        }),
    )?;

    Ok(saved)
}

pub fn list_preference_dividends<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, PREFERENCE_DIVIDEND).list(scope, 1000)?;
    let filtered: Vec<Value> = match company_id {
        Some(company) => rows
            .into_iter()
            .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company))
            .collect(),
        None => rows,
    };
    Ok(json!({ "dividends": filtered }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn an_ordinary_dividend_is_per_share_over_the_shares_in_issue() {
        let graph = FakeGraph::returning(vec![json!({"id": "d-1"})]);
        declare_ordinary_dividend(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1", "dividend_type": "interim",
                "per_share_amount": "0.35", "total_shares": 1000,
                "record_date": "2026-03-01"
            }),
        )
        .expect("declared");
        let (_, params) = graph.last();
        let written = &params["props"];
        assert_eq!(written["total_dividend"], json!("350.000000"));
        assert_eq!(written["status"], json!("declared"));
        // the entry balances: retained earnings debited, payable credited
        let lines = written["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["debit"], json!("350.000000"));
        assert_eq!(lines[1]["credit"], json!("350.000000"));
    }

    #[test]
    fn a_dividend_per_share_of_a_huge_amount_keeps_every_digit() {
        let graph = FakeGraph::returning(vec![json!({"id": "d-1"})]);
        declare_ordinary_dividend(
            &graph,
            &scope(),
            &json!({"per_share_amount": "123456789012345678901.55", "total_shares": 3}),
        )
        .expect("declared");
        let (_, params) = graph.last();
        assert_eq!(
            params["props"]["total_dividend"],
            json!("370370367037037036704.650000")
        );
    }

    #[test]
    fn paying_marks_it_paid_and_keeps_the_release_entry() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "d-1", "dividend_type": "final", "total_dividend": "350", "status": "declared"
        })]);
        let answer = pay_ordinary_dividend(
            &graph,
            &scope(),
            "d-1",
            &json!({"payment_date": "2026-04-01"}),
        )
        .expect("paid");
        assert_eq!(answer["id"], json!("d-1"));
        let (_, params) = graph.last();
        assert_eq!(params["patch"]["status"], json!("paid"));
        assert_eq!(params["patch"]["payment_date"], json!("2026-04-01"));
        assert_eq!(
            params["patch"]["payment_journal_entry"]["entries"][0]["debit"],
            json!("350.000000")
        );
    }

    #[test]
    fn a_preference_declaration_adds_the_arrears() {
        let graph = FakeGraph::returning(vec![json!({"id": "c-1", "name": "A"})]);
        declare_preference_dividend(
            &graph,
            &scope(),
            "c-1",
            &json!({
                "company_id": "co-1", "per_share_amount": "2", "total_shares": 100,
                "preference_arears": "50.25", "record_date": "2026-03-01"
            }),
        )
        .expect("declared");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["total_dividend"], json!("250.250000"));
        assert_eq!(params["props"]["dividend_type"], json!("fixed"));
    }

    #[test]
    fn negative_arrears_credit_the_declaration_because_the_service_never_refused_them() {
        let graph = FakeGraph::returning(vec![json!({"id": "c-1", "name": "A"})]);
        declare_preference_dividend(
            &graph,
            &scope(),
            "c-1",
            &json!({"per_share_amount": "2", "total_shares": 100, "preference_arears": "-50"}),
        )
        .expect("declared");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["total_dividend"], json!("150.000000"));
    }

    #[test]
    fn issuing_preference_shares_splits_price_from_nominal() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "c-1", "name": "A", "issue_price": "2.50", "nominal_value": "1",
            "shares_issued": 100, "shares_outstanding": 100
        })]);
        issue_preference_shares(&graph, &scope(), "c-1", &json!({"shares_issued": 50}))
            .expect("issued");
        let (_, params) = graph.last();
        assert_eq!(params["patch"]["shares_issued"], json!(150));
        assert_eq!(params["patch"]["shares_outstanding"], json!(150));
        let lines = params["patch"]["last_issue_journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["debit"], json!("125.000000"));
        assert_eq!(lines[1]["credit"], json!("50.000000"));
        assert_eq!(lines[2]["credit"], json!("75.000000"));
    }

    #[test]
    fn redeeming_sends_the_excess_over_nominal_to_the_redemption_reserve() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "c-1", "name": "A", "nominal_value": "1",
            "shares_issued": 200, "shares_outstanding": 200
        })]);
        let answer = redeem_preference_shares(
            &graph,
            &scope(),
            "c-1",
            &json!({"shares_redeemed": 100, "redemption_price": "1.75"}),
        )
        .expect("redeemed");
        assert_eq!(answer["id"], json!("c-1"));
        // the class is read, the redemption written, then the outstanding count corrected
        let (_, write) = graph.statement(1);
        let lines = write["props"]["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["debit"], json!("100.000000"));
        assert_eq!(lines[1]["debit"], json!("75.000000"));
        assert_eq!(lines[2]["credit"], json!("175.000000"));
        let (_, params) = graph.last();
        assert_eq!(params["patch"]["shares_outstanding"], json!(100));
    }

    #[test]
    fn redeeming_more_than_is_outstanding_still_records_and_goes_negative() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "c-1", "name": "A", "nominal_value": "1",
            "shares_issued": 100, "shares_outstanding": 40
        })]);
        redeem_preference_shares(
            &graph,
            &scope(),
            "c-1",
            &json!({"shares_redeemed": 100, "redemption_price": "1"}),
        )
        .expect("redeemed");
        let (_, params) = graph.last();
        assert_eq!(params["patch"]["shares_outstanding"], json!(-60));
    }
}
