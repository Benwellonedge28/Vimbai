#![forbid(unsafe_code)]
//! share-redemption-service, on the store.
//!
//! A redemption is a process, not a single entry, so it is modelled as one: it is initiated, possibly
//! funded by a fresh issue, then completed once the statutory declaration is made. The reserve it must
//! create depends on where the money came from.
//!
//! The arithmetic, all exact:
//!
//! * what is paid is shares times redemption price,
//! * redeeming out of proceeds sets up a requirement for the nominal value of the shares, with the
//!   proceeds to be filled in later - so the requirement exists before it can be judged,
//! * completing a redemption funded by a fresh issue creates no reserve at all, because the capital
//!   replaced the capital; every other method creates the nominal value,
//! * a fresh issue splits its proceeds into share capital at nominal and premium above it.
//!
//! Kept as shipped:
//!
//! * A redemption funded by proceeds starts `awaiting_crr`; the other methods start at their model's
//!   own default and are not moved by the initiation.
//! * The fresh-issue endpoint does not update the requirement it was recorded against, so the
//!   requirement keeps the proceeds it was created with. That is a gap in the service, and closing it
//!   here would change what a compliance query answers.
//! * Preference capital is `3205`; every other class `3200`.
//!
//! One deliberate change: a redemption that does not exist is a refusal, not a 200 with an error body,
//! on both the fresh-issue and complete paths.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, CRR_REQUIREMENT, FRESH_ISSUE, SHARE_REDEMPTION,
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

/// What a redemption of this many shares costs.
pub fn total_value(price: &Money, shares: i64) -> Money {
    price.times(&Money::from_i64(shares))
}

/// The reserve a completed redemption must create: none when a fresh issue funded it, the nominal
/// value otherwise.
pub fn reserve_created(method: &str, nominal: &Money, shares: i64) -> Money {
    if method == "fresh_issue" {
        Money::zero()
    } else {
        nominal.times(&Money::from_i64(shares))
    }
}

/// `/redemptions/initiate`.
pub fn initiate_redemption<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let shares = count(payload, "shares_redeemed");
    let nominal = amount_or_zero(payload, "nominal_value")?;
    let price = amount_or_zero(payload, "redemption_price")?;
    let method = text(payload, "redemption_method");
    let share_class = text(payload, "share_class");
    let total = total_value(&price, shares);
    let nominal_total = nominal.times(&Money::from_i64(shares));

    let redemption = Records::new(graph, SHARE_REDEMPTION).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "share_class": share_class,
            "shares_redeemed": shares,
            "nominal_value": wire(&nominal),
            "redemption_price": wire(&price),
            "redemption_date": text(payload, "redemption_date"),
            "redemption_method": method,
            "authority_date": text(payload, "authority_date"),
            "total_redemption_value": wire(&total),
            "status": if method == "proceeds" { "awaiting_crr" } else { "initiated" },
        }),
    )?;

    if method == "proceeds" {
        let id = redemption
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        Records::new(graph, CRR_REQUIREMENT).create(
            scope,
            &json!({
                "redemption_id": id,
                "nominal_value_of_shares": wire(&nominal_total),
                "proceeds_used": wire(&total),
                "fresh_issue_proceeds": wire(&zero()),
                "minimum_crr_required": wire(&nominal_total),
                "crr_created": wire(&zero()),
                "source_of_crr": "requires_calculation",
            }),
        )?;
    }

    Ok(redemption)
}

/// `/redemptions/{id}/fresh-issue`.
pub fn record_fresh_issue<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    redemption_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    Records::new(graph, SHARE_REDEMPTION).get(scope, redemption_id)?;

    let shares = count(payload, "shares_issued");
    let price = amount_or_zero(payload, "issue_price")?;
    let nominal = amount_or_zero(payload, "nominal_value")?;
    let proceeds = price.times(&Money::from_i64(shares));
    let capital = nominal.times(&Money::from_i64(shares));
    let premium = proceeds.minus(&capital);

    let journal = ledger(
        &text(payload, "issue_date"),
        "Fresh issue to fund redemption".to_string(),
        "FRESH-ISS".to_string(),
        vec![
            line("1000", "Bank", &proceeds, &zero()),
            line("3200", "Share Capital", &zero(), &capital),
            line("3210", "Share Premium", &zero(), &premium),
        ],
    );

    Records::new(graph, FRESH_ISSUE).create(
        scope,
        &json!({
            "redemption_id": redemption_id,
            "shares_issued": shares,
            "issue_price": wire(&price),
            "nominal_value": wire(&nominal),
            "issue_date": text(payload, "issue_date"),
            "total_proceeds": wire(&proceeds),
            "journal_entry": journal,
        }),
    )
}

/// `/redemptions/{id}/complete`.
pub fn complete_redemption<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    redemption_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, SHARE_REDEMPTION);
    let redemption = records.get(scope, redemption_id)?;

    let shares = redemption
        .get("shares_redeemed")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let nominal = stored_amount(&redemption, "nominal_value")?;
    let total = stored_amount(&redemption, "total_redemption_value")?;
    let method = redemption
        .get("redemption_method")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let share_class = redemption
        .get("share_class")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let capital = nominal.times(&Money::from_i64(shares));
    let reserve = reserve_created(&method, &nominal, shares);

    let mut lines = vec![line(
        capital_account(&share_class),
        "Share Capital",
        &capital,
        &zero(),
    )];
    if reserve > Money::zero() {
        lines.push(line(
            "3220",
            "Capital Redemption Reserve",
            &reserve,
            &zero(),
        ));
    }
    lines.push(line("1000", "Bank", &zero(), &total));

    let date = redemption
        .get("redemption_date")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let journal = ledger(
        &date,
        format!("Redemption of {shares} {share_class} shares"),
        format!("SH-RED-{redemption_id}"),
        lines,
    );

    let saved = records.update(
        scope,
        redemption_id,
        &json!({
            "statutory_declaration_date": text(payload, "statutory_declaration_date"),
            "journal_entry": journal,
            "crr_created": wire(&reserve),
            "status": "completed",
        }),
    )?;

    Ok(json!({
        "redemption": saved,
        "crr_created": wire(&reserve),
    }))
}

/// `/redemptions`, optionally by company and by status.
pub fn list_redemptions<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
    status: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, SHARE_REDEMPTION).list(scope, 1000)?;
    let filtered: Vec<Value> = rows
        .into_iter()
        .filter(|row| match company_id {
            Some(company) => row.get("company_id").and_then(Value::as_str) == Some(company),
            None => true,
        })
        .filter(|row| match status {
            Some(wanted) => row.get("status").and_then(Value::as_str) == Some(wanted),
            None => true,
        })
        .collect();
    Ok(json!({ "redemptions": filtered }))
}

/// `/redemptions/{id}`: the redemption with what funded it and what it owes the reserve.
pub fn redemption_detail<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    redemption_id: &str,
) -> Result<Value, StoreError> {
    let redemption = Records::new(graph, SHARE_REDEMPTION).get(scope, redemption_id)?;
    let mine = |rows: Vec<Value>| -> Vec<Value> {
        rows.into_iter()
            .filter(|row| row.get("redemption_id").and_then(Value::as_str) == Some(redemption_id))
            .collect()
    };
    let fresh_issues = mine(Records::new(graph, FRESH_ISSUE).list(scope, 1000)?);
    let requirements = mine(Records::new(graph, CRR_REQUIREMENT).list(scope, 1000)?);
    Ok(json!({
        "redemption": redemption,
        "fresh_issues": fresh_issues,
        "crr_requirement": requirements.first().cloned().unwrap_or(Value::Null),
    }))
}

/// `/crr-requirements`, optionally by compliance status.
pub fn list_crr_requirements<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    status: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, CRR_REQUIREMENT).list(scope, 1000)?;
    let filtered: Vec<Value> = match status {
        Some(wanted) => rows
            .into_iter()
            .filter(|row| row.get("compliance_status").and_then(Value::as_str) == Some(wanted))
            .collect(),
        None => rows,
    };
    Ok(json!({ "crr_requirements": filtered }))
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
    fn redeeming_out_of_proceeds_looks_for_the_nominal_value() {
        let reserve = reserve_created("proceeds", &money("1"), 500);
        assert_eq!(wire(&reserve), "500.000000");
    }

    #[test]
    fn a_fresh_issue_funds_the_redemption_so_no_reserve_is_needed() {
        let reserve = reserve_created("fresh_issue", &money("1"), 500);
        assert_eq!(wire(&reserve), "0.000000");
    }

    #[test]
    fn initiating_out_of_proceeds_sets_up_the_requirement_and_waits_for_it() {
        // two keys, because a single-key row is unwrapped to its value - a real node carries more
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "red-1", "status": "awaiting_crr"})],
            vec![json!({"id": "req-1"})],
        ]);
        initiate_redemption(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1", "share_class": "ordinary", "shares_redeemed": 500,
                "nominal_value": "1", "redemption_price": "1.40", "redemption_method": "proceeds",
                "redemption_date": "2026-03-01", "authority_date": "2026-02-01"
            }),
        )
        .expect("initiated");
        let (_, created) = graph.statement(0);
        assert_eq!(
            created["props"]["total_redemption_value"],
            json!("700.000000")
        );
        assert_eq!(created["props"]["status"], json!("awaiting_crr"));
        let (_, requirement) = graph.statement(1);
        assert_eq!(
            requirement["props"]["minimum_crr_required"],
            json!("500.000000")
        );
        assert_eq!(requirement["props"]["redemption_id"], json!("red-1"));
    }

    #[test]
    fn initiating_a_fresh_issue_redemption_writes_no_requirement() {
        let graph = FakeGraph::returning(vec![json!({"id": "red-2"})]);
        initiate_redemption(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1", "share_class": "preference", "shares_redeemed": 100,
                "nominal_value": "1", "redemption_price": "1", "redemption_method": "fresh_issue",
                "redemption_date": "2026-03-01", "authority_date": "2026-02-01"
            }),
        )
        .expect("initiated");
        assert_eq!(graph.statements(), 1);
    }

    #[test]
    fn a_fresh_issue_splits_its_proceeds_into_capital_and_premium() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "red-1"})],
            vec![json!({"id": "f-1"})],
        ]);
        record_fresh_issue(
            &graph,
            &scope(),
            "red-1",
            &json!({"shares_issued": 500, "issue_price": "1.40", "nominal_value": "1",
                    "issue_date": "2026-02-15"}),
        )
        .expect("recorded");
        let (_, params) = graph.statement(1);
        let written = &params["props"];
        assert_eq!(written["total_proceeds"], json!("700.000000"));
        let lines = written["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[1]["credit"], json!("500.000000"));
        assert_eq!(lines[2]["credit"], json!("200.000000"));
    }

    #[test]
    fn completing_a_fresh_issue_redemption_creates_no_reserve_line() {
        let graph = FakeGraph::queue(vec![
            vec![json!({
                "id": "red-1", "shares_redeemed": 500, "nominal_value": "1",
                "total_redemption_value": "700", "redemption_method": "fresh_issue",
                "share_class": "ordinary", "redemption_date": "2026-03-01"
            })],
            vec![json!({"id": "red-1"})],
        ]);
        let answer = complete_redemption(&graph, &scope(), "red-1", &json!({})).expect("completed");
        assert_eq!(answer["crr_created"], json!("0.000000"));
        let (_, params) = graph.statement(1);
        let lines = params["patch"]["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1]["account_code"], json!("1000"));
    }

    #[test]
    fn completing_a_proceeds_redemption_creates_the_nominal_reserve() {
        let graph = FakeGraph::queue(vec![
            vec![json!({
                "id": "red-1", "shares_redeemed": 500, "nominal_value": "1",
                "total_redemption_value": "700", "redemption_method": "proceeds",
                "share_class": "preference", "redemption_date": "2026-03-01"
            })],
            vec![json!({"id": "red-1"})],
        ]);
        let answer = complete_redemption(&graph, &scope(), "red-1", &json!({})).expect("completed");
        assert_eq!(answer["crr_created"], json!("500.000000"));
        let (_, params) = graph.statement(1);
        let lines = params["patch"]["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["account_code"], json!("3205"));
        assert_eq!(lines[1]["debit"], json!("500.000000"));
    }

    #[test]
    fn a_redemption_with_more_digits_than_a_double_holds_keeps_them() {
        let graph = FakeGraph::queue(vec![
            vec![json!({
                "id": "red-1", "shares_redeemed": 2, "nominal_value": "0.45",
                "total_redemption_value": "246913578024691356802.20",
                "redemption_method": "proceeds", "share_class": "ordinary",
                "redemption_date": "2026-03-01"
            })],
            vec![json!({"id": "red-1"})],
        ]);
        let answer = complete_redemption(&graph, &scope(), "red-1", &json!({})).expect("completed");
        assert_eq!(answer["crr_created"], json!("0.900000"));
    }
}
