#![forbid(unsafe_code)]
//! capital-redemption-reserve-service, on the store.
//!
//! A capital redemption reserve is what a company must hold when it buys back its own shares, so the
//! capital it hands back does not simply leave the balance sheet. It is created either by the redemption
//! itself or by a capital reduction, and it is spent on a bonus issue, written off, or transferred out.
//!
//! The redemption arithmetic is the interesting part, and it is exact: the money paid is shares times
//! redemption price, and the reserve created is that less the nominal value given back. So a buy-back at
//! nominal creates no reserve, and a buy-back above nominal creates exactly the premium paid.
//!
//! Kept as shipped:
//!
//! * Preference shares redeem against `3205`; every other class, including an unrecognised one, against
//!   ordinary share capital `3200`.
//! * A utilization is `bonus_issue` (moves to share capital `3200`), `write_off` (to retained earnings
//!   `3300`), or anything else, which transfers to general reserve `3310`.
//! * The summary counts creations and redemptions together as one figure, and reports how many records
//!   of all three kinds the company has.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, CRR_CREATION, CRR_UTILIZATION, REDEMPTION_TRANSACTION,
};

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn count(payload: &Value, key: &str) -> i64 {
    payload
        .get(key)
        .and_then(|value| match value {
            Value::Number(number) => number.as_i64(),
            Value::String(raw) => raw.trim().parse::<i64>().ok(),
            _ => None,
        })
        .unwrap_or(0)
}

fn ledger(date: &str, description: String, reference: String, lines: Vec<Value>) -> Value {
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

/// The capital account a redemption of this class debits.
pub fn capital_account(share_class: &str) -> &'static str {
    if share_class == "preference" {
        "3205"
    } else {
        "3200"
    }
}

/// Proceeds paid for the redeemed shares.
pub fn redemption_proceeds(price: &Money, shares: i64) -> Money {
    price.times(&Money::from_i64(shares))
}

/// The reserve a redemption must create: what was paid, less the nominal handed back.
pub fn redemption_reserve(price: &Money, nominal: &Money, shares: i64) -> Money {
    redemption_proceeds(price, shares).minus(&nominal.times(&Money::from_i64(shares)))
}

/// Where a utilization of the reserve lands.
pub fn utilization_destination(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "bonus_issue" => ("3200", "Share Capital"),
        "write_off" => ("3300", "Retained Earnings"),
        _ => ("3310", "General Reserve"),
    }
}

/// `/redemptions/record`.
pub fn record_redemption<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let shares = count(payload, "shares_redeemed");
    let price = amount_or_zero(payload, "redemption_price")?;
    let nominal = amount_or_zero(payload, "nominal_value")?;
    let share_class = text(payload, "share_class");
    let date = text(payload, "redemption_date");

    let proceeds = redemption_proceeds(&price, shares);
    let reserve = redemption_reserve(&price, &nominal, shares);
    let capital = nominal.times(&Money::from_i64(shares));
    let account = capital_account(&share_class);

    let journal = ledger(
        &date,
        format!("Redemption of {shares} {share_class} shares - CRR created"),
        "CRR-RED".to_string(),
        vec![
            line(
                account,
                &format!("{} Share Capital", title_case(&share_class)),
                &capital,
                &zero(),
            ),
            line("3220", "Capital Redemption Reserve", &reserve, &zero()),
            line("1000", "Bank", &zero(), &proceeds),
        ],
    );

    Records::new(graph, REDEMPTION_TRANSACTION).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "share_class": share_class,
            "shares_redeemed": shares,
            "redemption_price": wire(&price),
            "nominal_value": wire(&nominal),
            "redemption_date": date,
            "source_account": text(payload, "source_account"),
            "total_proceeds": wire(&proceeds),
            "redemption_reserve_amount": wire(&reserve),
            "journal_entry": journal,
        }),
    )
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// `/creations/create`: a capital reduction sets the reserve up directly.
pub fn create_crr<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let amount = amount_or_zero(payload, "amount")?;
    let description = text(payload, "description");
    let date = text(payload, "creation_date");

    let journal = ledger(
        &date,
        format!("Creation of Capital Redemption Reserve: {description}"),
        "CRR-CREATE".to_string(),
        vec![
            line("3300", "Retained Earnings / P&L", &amount, &zero()),
            line("3220", "Capital Redemption Reserve", &zero(), &amount),
        ],
    );

    Records::new(graph, CRR_CREATION).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "amount": wire(&amount),
            "source": text(payload, "source"),
            "description": description,
            "creation_date": date,
            "journal_entry": journal,
        }),
    )
}

/// `/utilizations/record`.
pub fn utilize_crr<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let amount = amount_or_zero(payload, "amount")?;
    let kind = text(payload, "utilization_type");
    let description = text(payload, "description");
    let date = text(payload, "utilization_date");
    let (code, name) = utilization_destination(&kind);

    let label = match kind.as_str() {
        "bonus_issue" => format!("CRR utilized for bonus issue: {description}"),
        "write_off" => format!("CRR written off: {description}"),
        _ => format!("CRR transferred: {description}"),
    };

    let journal = ledger(
        &date,
        label,
        "CRR-UTIL".to_string(),
        vec![
            line("3220", "Capital Redemption Reserve", &amount, &zero()),
            line(code, name, &zero(), &amount),
        ],
    );

    Records::new(graph, CRR_UTILIZATION).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "amount": wire(&amount),
            "utilization_type": kind,
            "description": description,
            "utilization_date": date,
            "journal_entry": journal,
        }),
    )
}

fn company_rows<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    kind: vimbai_store::RecordKind,
    company_id: Option<&str>,
) -> Result<Vec<Value>, StoreError> {
    let rows = Records::new(graph, kind).list(scope, 1000)?;
    Ok(match company_id {
        Some(company) => rows
            .into_iter()
            .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company))
            .collect(),
        None => rows,
    })
}

pub fn list_redemptions<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    Ok(Value::Array(company_rows(
        graph,
        scope,
        REDEMPTION_TRANSACTION,
        company_id,
    )?))
}

pub fn list_creations<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    Ok(Value::Array(company_rows(
        graph,
        scope,
        CRR_CREATION,
        company_id,
    )?))
}

pub fn list_utilizations<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    Ok(Value::Array(company_rows(
        graph,
        scope,
        CRR_UTILIZATION,
        company_id,
    )?))
}

/// `/summary/{company_id}`: everything created, less everything used.
pub fn crr_summary<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let redemptions = company_rows(graph, scope, REDEMPTION_TRANSACTION, Some(company_id))?;
    let creations = company_rows(graph, scope, CRR_CREATION, Some(company_id))?;
    let utilizations = company_rows(graph, scope, CRR_UTILIZATION, Some(company_id))?;

    let mut created = Money::zero();
    for row in &redemptions {
        created = created.plus(&stored_amount(row, "redemption_reserve_amount")?);
    }
    for row in &creations {
        created = created.plus(&stored_amount(row, "amount")?);
    }
    let mut used = Money::zero();
    for row in &utilizations {
        used = used.plus(&stored_amount(row, "amount")?);
    }
    let balance = created.minus(&used);

    Ok(json!({
        "company_id": company_id,
        "total_created": wire(&created),
        "total_utilized": wire(&used),
        "current_balance": wire(&balance),
        "transaction_count": redemptions.len() + creations.len() + utilizations.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn money(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    #[test]
    fn a_buy_back_at_nominal_creates_no_reserve() {
        let reserve = redemption_reserve(&money("1"), &money("1"), 500);
        assert_eq!(wire(&reserve), "0.000000");
    }

    #[test]
    fn a_buy_back_above_nominal_creates_exactly_the_premium() {
        let reserve = redemption_reserve(&money("1.40"), &money("1"), 500);
        assert_eq!(wire(&reserve), "200.000000");
    }

    #[test]
    fn a_huge_buy_back_keeps_every_digit() {
        let reserve = redemption_reserve(&money("123456789012345678901.55"), &money("0.45"), 2);
        assert_eq!(wire(&reserve), "246913578024691357802.200000");
    }

    #[test]
    fn preference_capital_is_a_different_account() {
        assert_eq!(capital_account("preference"), "3205");
        assert_eq!(capital_account("ordinary"), "3200");
        assert_eq!(capital_account(""), "3200");
    }

    #[test]
    fn the_redemption_entry_debits_capital_and_the_reserve_and_pays_bank() {
        let graph = FakeGraph::returning(vec![json!({"id": "t-1"})]);
        record_redemption(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1", "share_class": "preference", "shares_redeemed": 500,
                "redemption_price": "1.40", "nominal_value": "1",
                "redemption_date": "2026-03-01", "source_account": "distributable_profits"
            }),
        )
        .expect("recorded");
        let (_, params) = graph.last();
        let written = &params["props"];
        assert_eq!(written["total_proceeds"], json!("700.000000"));
        assert_eq!(written["redemption_reserve_amount"], json!("200.000000"));
        let lines = written["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["account_code"], json!("3205"));
        assert_eq!(lines[0]["debit"], json!("500.000000"));
        assert_eq!(lines[1]["debit"], json!("200.000000"));
        assert_eq!(lines[2]["credit"], json!("700.000000"));
    }

    #[test]
    fn each_utilization_kind_lands_where_the_service_sent_it() {
        assert_eq!(
            utilization_destination("bonus_issue"),
            ("3200", "Share Capital")
        );
        assert_eq!(
            utilization_destination("write_off"),
            ("3300", "Retained Earnings")
        );
        assert_eq!(
            utilization_destination("transfer"),
            ("3310", "General Reserve")
        );
        assert_eq!(
            utilization_destination("anything"),
            ("3310", "General Reserve")
        );
    }

    #[test]
    fn the_summary_counts_redemptions_and_creations_as_one_figure() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"company_id": "co-1", "redemption_reserve_amount": "200"})],
            vec![json!({"company_id": "co-1", "amount": "300.50"})],
            vec![json!({"company_id": "co-1", "amount": "100"})],
        ]);
        let answer = crr_summary(&graph, &scope(), "co-1").expect("summary");
        assert_eq!(answer["total_created"], json!("500.500000"));
        assert_eq!(answer["total_utilized"], json!("100.000000"));
        assert_eq!(answer["current_balance"], json!("400.500000"));
        assert_eq!(answer["transaction_count"], json!(3));
    }

    #[test]
    fn another_companys_reserve_never_enters_the_summary() {
        let graph = FakeGraph::queue(vec![
            Vec::new(),
            vec![json!({"company_id": "co-2", "amount": "999"})],
            Vec::new(),
        ]);
        let answer = crr_summary(&graph, &scope(), "co-1").expect("summary");
        assert_eq!(answer["total_created"], json!("0.000000"));
        assert_eq!(answer["transaction_count"], json!(0));
    }
}
