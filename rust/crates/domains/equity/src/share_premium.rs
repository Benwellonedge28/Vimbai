#![forbid(unsafe_code)]
//! share-premium-service, on the store.
//!
//! Premium is what a share was sold for above its nominal value. Three records move it: the issue that
//! created it, the spending of it, and a correction.
//!
//! The issue arithmetic is one line and it is exact: premium is (price less nominal) times shares, and
//! the bank receives shares times price. The two add up only because the issue price is split into the
//! nominal part that goes to share capital and the rest that goes to premium.
//!
//! Kept as shipped:
//!
//! * An issue below nominal produces a negative premium rather than a refusal. It is a discount on
//!   issue, which the service recorded and which a records system should show rather than reject.
//! * A utilization is one of four kinds. Bonus issue moves premium to share capital; write off moves it
//!   to retained earnings; merger expenses and anything else, including an unknown kind, credit 4100
//!   as an expense. The catch-all is the service's own `else` branch.
//! * An adjustment is signed: a positive one credits premium and debits retained earnings, a negative
//!   one does the reverse.
//! * The balance is premium received, less what was used, plus adjustments.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, PREMIUM_ADJUSTMENT, PREMIUM_UTILIZATION,
    SHARE_PREMIUM_ENTRY,
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

/// Premium on one issue: (price less nominal) times shares.
pub fn premium_on_issue(issue_price: &Money, nominal_value: &Money, shares: i64) -> Money {
    let per_share = issue_price.minus(nominal_value);
    per_share.times(&Money::from_i64(shares))
}

/// Where a utilization of premium lands.
pub fn utilization_destination(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "bonus_issue" => ("3200", "Share Capital"),
        "write_off" => ("3300", "Retained Earnings"),
        "merger_expenses" => ("4100", "Administrative Expenses"),
        _ => ("4100", "Legal Expenses"),
    }
}

/// `/entries/record`.
pub fn record_share_premium<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let shares = count(payload, "shares_issued");
    let nominal = amount_or_zero(payload, "nominal_value")?;
    let price = amount_or_zero(payload, "issue_price")?;
    let premium = premium_on_issue(&price, &nominal, shares);
    let share_class = text(payload, "share_class");
    let date = text(payload, "entry_date");

    let bank = price.times(&Money::from_i64(shares));
    let capital = nominal.times(&Money::from_i64(shares));

    let journal = ledger(
        &date,
        format!("Share premium from issue of {shares} {share_class} shares"),
        format!("SP-{}", text(payload, "reference_id")),
        vec![
            line("1000", "Bank", &bank, &zero()),
            line("3200", "Share Capital", &zero(), &capital),
            line("3210", "Share Premium", &zero(), &premium),
        ],
    );

    Records::new(graph, SHARE_PREMIUM_ENTRY).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "entry_type": text(payload, "entry_type"),
            "shares_issued": shares,
            "nominal_value": wire(&nominal),
            "issue_price": wire(&price),
            "premium_amount": wire(&premium),
            "share_class": share_class,
            "reference_id": text(payload, "reference_id"),
            "entry_date": date,
            "journal_entry": journal,
        }),
    )
}

/// `/utilizations/record`.
pub fn utilize_premium<G: Graph + ?Sized>(
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
        "bonus_issue" => format!("Share premium utilized for bonus issue: {description}"),
        "write_off" => format!("Share premium written off: {description}"),
        "merger_expenses" => format!("Share premium for merger expenses: {description}"),
        _ => format!("Share premium for legal costs: {description}"),
    };

    let journal = ledger(
        &date,
        label,
        "SP-UTIL".to_string(),
        vec![
            line("3210", "Share Premium", &amount, &zero()),
            line(code, name, &zero(), &amount),
        ],
    );

    Records::new(graph, PREMIUM_UTILIZATION).create(
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

/// `/adjustments/create`: a correction, by sign.
pub fn adjust_premium<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let adjustment = amount_or_zero(payload, "adjustment_amount")?;
    let original = amount_or_zero(payload, "original_amount")?;
    let description = text(payload, "description");
    let date = text(payload, "adjustment_date");
    let size = adjustment.abs();

    let (premium_debit, premium_credit, earnings_debit, earnings_credit) =
        if adjustment > Money::zero() {
            (zero(), size.clone(), size.clone(), zero())
        } else {
            (size.clone(), zero(), zero(), size.clone())
        };

    let journal = ledger(
        &date,
        format!("Share premium adjustment: {description}"),
        "SP-ADJ".to_string(),
        vec![
            line("3210", "Share Premium", &premium_debit, &premium_credit),
            line(
                "3300",
                "Retained Earnings",
                &earnings_debit,
                &earnings_credit,
            ),
        ],
    );

    Records::new(graph, PREMIUM_ADJUSTMENT).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "adjustment_type": text(payload, "adjustment_type"),
            "original_amount": wire(&original),
            "adjustment_amount": wire(&adjustment),
            "description": description,
            "adjustment_date": date,
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

pub fn list_entries<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    Ok(Value::Array(company_rows(
        graph,
        scope,
        SHARE_PREMIUM_ENTRY,
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
        PREMIUM_UTILIZATION,
        company_id,
    )?))
}

pub fn list_adjustments<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    Ok(Value::Array(company_rows(
        graph,
        scope,
        PREMIUM_ADJUSTMENT,
        company_id,
    )?))
}

/// `/summary/{company_id}`: received, less used, plus adjusted.
pub fn premium_summary<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let entries = company_rows(graph, scope, SHARE_PREMIUM_ENTRY, Some(company_id))?;
    let utilizations = company_rows(graph, scope, PREMIUM_UTILIZATION, Some(company_id))?;
    let adjustments = company_rows(graph, scope, PREMIUM_ADJUSTMENT, Some(company_id))?;

    let mut received = Money::zero();
    for row in &entries {
        received = received.plus(&stored_amount(row, "premium_amount")?);
    }
    let mut used = Money::zero();
    for row in &utilizations {
        used = used.plus(&stored_amount(row, "amount")?);
    }
    let mut adjusted = Money::zero();
    for row in &adjustments {
        adjusted = adjusted.plus(&stored_amount(row, "adjustment_amount")?);
    }
    let balance = received.minus(&used).plus(&adjusted);

    Ok(json!({
        "company_id": company_id,
        "total_premium_received": wire(&received),
        "total_utilized": wire(&used),
        "total_adjusted": wire(&adjusted),
        "current_balance": wire(&balance),
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
    fn premium_is_the_excess_over_nominal_times_shares() {
        let premium = premium_on_issue(&money("2.25"), &money("1"), 1000);
        assert_eq!(wire(&premium), "1250.000000");
    }

    #[test]
    fn a_huge_issue_keeps_every_digit() {
        let premium = premium_on_issue(&money("123456789012345678901.55"), &money("0.45"), 3);
        assert_eq!(wire(&premium), "370370367037037036703.300000");
    }

    #[test]
    fn an_issue_below_nominal_makes_a_negative_premium_rather_than_failing() {
        let premium = premium_on_issue(&money("0.80"), &money("1"), 100);
        assert_eq!(wire(&premium), "-20.000000");
    }

    #[test]
    fn the_issue_entry_splits_the_price_into_capital_and_premium() {
        let graph = FakeGraph::returning(vec![json!({"id": "sp-1"})]);
        record_share_premium(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1", "shares_issued": 1000, "nominal_value": "1",
                "issue_price": "2.25", "share_class": "ordinary", "reference_id": "iss-9"
            }),
        )
        .expect("recorded");
        let (_, params) = graph.last();
        let written = &params["props"];
        assert_eq!(written["premium_amount"], json!("1250.000000"));
        let lines = written["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["debit"], json!("2250.000000"));
        assert_eq!(lines[1]["credit"], json!("1000.000000"));
        assert_eq!(lines[2]["credit"], json!("1250.000000"));
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
            utilization_destination("merger_expenses"),
            ("4100", "Administrative Expenses")
        );
        assert_eq!(
            utilization_destination("legal_costs"),
            ("4100", "Legal Expenses")
        );
        // and anything else is a legal cost, as the else branch was
        assert_eq!(
            utilization_destination("something_new"),
            ("4100", "Legal Expenses")
        );
    }

    #[test]
    fn a_bonus_issue_moves_premium_into_share_capital() {
        let graph = FakeGraph::returning(vec![json!({"id": "u-1"})]);
        utilize_premium(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "amount": "500", "utilization_type": "bonus_issue",
                    "description": "1 for 4"}),
        )
        .expect("recorded");
        let (_, params) = graph.last();
        let lines = params["props"]["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["debit"], json!("500.000000"));
        assert_eq!(lines[1]["account_code"], json!("3200"));
        assert_eq!(lines[1]["credit"], json!("500.000000"));
    }

    #[test]
    fn a_positive_adjustment_credits_premium_and_a_negative_one_debits_it() {
        let graph = FakeGraph::returning(vec![json!({"id": "a-1"})]);
        adjust_premium(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "adjustment_amount": "75.5", "description": "understated"}),
        )
        .expect("recorded");
        let (_, params) = graph.last();
        let lines = params["props"]["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["credit"], json!("75.500000"));
        assert_eq!(lines[1]["debit"], json!("75.500000"));

        let down = FakeGraph::returning(vec![json!({"id": "a-2"})]);
        adjust_premium(
            &down,
            &scope(),
            &json!({"company_id": "co-1", "adjustment_amount": "-75.5", "description": "overstated"}),
        )
        .expect("recorded");
        let (_, params) = down.last();
        let lines = params["props"]["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["debit"], json!("75.500000"));
        assert_eq!(lines[1]["credit"], json!("75.500000"));
    }

    #[test]
    fn the_balance_is_received_less_used_plus_adjusted() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"company_id": "co-1", "premium_amount": "1250"})],
            vec![json!({"company_id": "co-1", "amount": "250.25"})],
            vec![json!({"company_id": "co-1", "adjustment_amount": "-0.75"})],
        ]);
        let answer = premium_summary(&graph, &scope(), "co-1").expect("summary");
        assert_eq!(answer["total_premium_received"], json!("1250.000000"));
        assert_eq!(answer["total_utilized"], json!("250.250000"));
        assert_eq!(answer["total_adjusted"], json!("-0.750000"));
        assert_eq!(answer["current_balance"], json!("999.000000"));
    }
}
