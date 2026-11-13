#![forbid(unsafe_code)]
//! general-reserve-service, on the store.
//!
//! A general reserve is a name against a balance. Money goes in from an allocation and comes out for
//! a purpose, and the balance is the running total of both - so an allocation and a utilization agree
//! with the reserve they move, rather than each keeping its own idea of the total.
//!
//! Kept as shipped:
//!
//! * Allocating from `retained_earnings` debits retained earnings (`3300`); anything else debits
//!   share premium (`3210`), including a source the service does not know.
//! * A utilization for `asset_purchase` or `working_capital` credits bank (`1000`); every other
//!   purpose credits dividend payable (`2310`), including an empty purpose.
//! * `minimum_balance` and `target_balance` are recorded and not enforced. The service stored them as
//!   the reserve's own terms and never used them to refuse anything, so a utilization that leaves the
//!   balance under the minimum is still recorded. Refusing it here would strand clients that set a
//!   target they never meant as a floor.
//!
//! One deliberate change: utilizing more than the balance exists is now a refusal. The service
//! answered 200 with `{"error": "Insufficient reserve balance"}` and wrote nothing, which reads as
//! success to a client that checks the status. The reserve cannot go negative either way.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::read::text;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, GENERAL_RESERVE, RESERVE_ALLOCATION, RESERVE_UTILIZATION,
};

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

/// The account an allocation draws on.
pub fn allocation_source_account(source: &str) -> &'static str {
    if source == "retained_earnings" {
        "3300"
    } else {
        "3210"
    }
}

/// The account a utilization pays out of.
pub fn utilization_destination_account(purpose: &str) -> &'static str {
    match purpose {
        "asset_purchase" | "working_capital" => "1000",
        _ => "2310",
    }
}

fn title_case(value: &str) -> String {
    value
        .replace('_', " ")
        .split(' ')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `/reserves/create`.
pub fn create_reserve<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "reserve_name");
    if name.trim().is_empty() {
        return Err(StoreError::Invalid("reserve_name is required".into()));
    }
    let initial = amount_or_zero(payload, "initial_balance")?;

    let journal = if initial > Money::zero() {
        Some(ledger(
            "",
            format!("Creation of {name} reserve"),
            format!("RESERVE-CREATE-{}", &text(payload, "id")),
            vec![
                line("3300", "Retained Earnings", &initial, &zero()),
                line("3310", "General Reserve", &zero(), &initial),
            ],
        ))
    } else {
        None
    };

    Records::new(graph, GENERAL_RESERVE).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "reserve_name": name,
            "description": text(payload, "description"),
            "current_balance": wire(&initial),
            "target_balance": payload.get("target_balance").cloned().unwrap_or(Value::Null),
            "minimum_balance": wire(&amount_or_zero(payload, "minimum_balance")?),
            "funding_source": payload
                .get("funding_source")
                .and_then(Value::as_str)
                .unwrap_or("retained_earnings"),
            "journal_entry": journal,
        }),
    )
}

/// `/reserves/{id}/allocate`.
pub fn allocate_to_reserve<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    reserve_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, GENERAL_RESERVE);
    let reserve = records.get(scope, reserve_id)?;
    let amount = amount_or_zero(payload, "amount")?;
    let source = text(payload, "source");
    let description = text(payload, "description");
    let date = text(payload, "allocation_date");
    let balance = stored_amount(&reserve, "current_balance")?;
    let updated = balance.plus(&amount);
    let name = reserve
        .get("reserve_name")
        .and_then(Value::as_str)
        .unwrap_or("general");

    let journal = ledger(
        &date,
        format!("Allocation to {name}: {description}"),
        format!("RESERVE-ALLOC-{reserve_id}"),
        vec![
            line(
                allocation_source_account(&source),
                &title_case(&source),
                &amount,
                &zero(),
            ),
            line("3310", "General Reserve", &zero(), &amount),
        ],
    );

    let allocation = Records::new(graph, RESERVE_ALLOCATION).create(
        scope,
        &json!({
            "reserve_id": reserve_id,
            "amount": wire(&amount),
            "allocation_date": date,
            "source": source,
            "description": description,
            "journal_entry": journal,
        }),
    )?;

    let saved = records.update(
        scope,
        reserve_id,
        &json!({
            "current_balance": wire(&updated),
            "updated_at": text(payload, "allocation_date"),
        }),
    )?;

    Ok(json!({
        "reserve": {
            "id": saved.get("id").cloned().unwrap_or_else(|| json!(reserve_id)),
            "current_balance": wire(&updated),
        },
        "allocation": allocation,
    }))
}

/// `/reserves/{id}/utilize`.
pub fn utilize_reserve<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    reserve_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, GENERAL_RESERVE);
    let reserve = records.get(scope, reserve_id)?;
    let amount = amount_or_zero(payload, "amount")?;
    let balance = stored_amount(&reserve, "current_balance")?;

    if amount > balance {
        return Err(StoreError::Invalid("Insufficient reserve balance".into()));
    }

    let purpose = text(payload, "purpose");
    let description = text(payload, "description");
    let date = text(payload, "utilization_date");
    let updated = balance.minus(&amount);
    let name = reserve
        .get("reserve_name")
        .and_then(Value::as_str)
        .unwrap_or("general");

    let journal = ledger(
        &date,
        format!("Utilization of {name}: {description}"),
        format!("RESERVE-UTIL-{reserve_id}"),
        vec![
            line("3310", "General Reserve", &amount, &zero()),
            line(
                utilization_destination_account(&purpose),
                &title_case(&purpose),
                &zero(),
                &amount,
            ),
        ],
    );

    let utilization = Records::new(graph, RESERVE_UTILIZATION).create(
        scope,
        &json!({
            "reserve_id": reserve_id,
            "amount": wire(&amount),
            "utilization_date": date,
            "purpose": purpose,
            "description": description,
            "journal_entry": journal,
        }),
    )?;

    let saved = records.update(
        scope,
        reserve_id,
        &json!({
            "current_balance": wire(&updated),
            "updated_at": text(payload, "utilization_date"),
        }),
    )?;

    Ok(json!({
        "reserve": {
            "id": saved.get("id").cloned().unwrap_or_else(|| json!(reserve_id)),
            "current_balance": wire(&updated),
        },
        "utilization": utilization,
    }))
}

pub fn list_reserves<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, GENERAL_RESERVE).list(scope, 1000)?;
    Ok(Value::Array(match company_id {
        Some(company) => rows
            .into_iter()
            .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company))
            .collect(),
        None => rows,
    }))
}

pub fn get_reserve<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    reserve_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, GENERAL_RESERVE).get(scope, reserve_id)
}

/// `/reserves/{id}/history`: both sides of the balance, each on its own.
pub fn reserve_history<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    reserve_id: &str,
) -> Result<Value, StoreError> {
    let mine = |rows: Vec<Value>| -> Vec<Value> {
        rows.into_iter()
            .filter(|row| row.get("reserve_id").and_then(Value::as_str) == Some(reserve_id))
            .collect()
    };
    let allocations = mine(Records::new(graph, RESERVE_ALLOCATION).list(scope, 1000)?);
    let utilizations = mine(Records::new(graph, RESERVE_UTILIZATION).list(scope, 1000)?);
    Ok(json!({
        "allocations": allocations,
        "utilizations": utilizations,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_reserve_opens_at_its_initial_balance_with_the_entry_that_created_it() {
        let graph = FakeGraph::returning(vec![json!({"id": "res-1"})]);
        create_reserve(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1", "reserve_name": "Plant", "initial_balance": "5000"
            }),
        )
        .expect("created");
        let (_, params) = graph.last();
        let written = &params["props"];
        assert_eq!(written["current_balance"], json!("5000.000000"));
        let lines = written["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["debit"], json!("5000.000000"));
        assert_eq!(lines[1]["credit"], json!("5000.000000"));
    }

    #[test]
    fn an_empty_reserve_opens_with_no_entry_at_all() {
        let graph = FakeGraph::returning(vec![json!({"id": "res-1"})]);
        create_reserve(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "reserve_name": "Plant"}),
        )
        .expect("created");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["journal_entry"], Value::Null);
    }

    #[test]
    fn allocating_adds_to_the_balance_and_debits_the_named_source() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "res-1", "reserve_name": "Plant", "current_balance": "5000"
        })]);
        let answer = allocate_to_reserve(
            &graph,
            &scope(),
            "res-1",
            &json!({"amount": "250.75", "source": "share_premium", "description": "top up"}),
        )
        .expect("allocated");
        assert_eq!(answer["reserve"]["current_balance"], json!("5250.750000"));
        let (_, write) = graph.statement(1);
        assert_eq!(
            write["props"]["journal_entry"]["entries"][0]["account_code"],
            json!("3210")
        );
        let (_, update) = graph.last();
        assert_eq!(update["patch"]["current_balance"], json!("5250.750000"));
    }

    #[test]
    fn allocating_from_retained_earnings_debits_retained_earnings() {
        assert_eq!(allocation_source_account("retained_earnings"), "3300");
        assert_eq!(allocation_source_account("share_premium"), "3210");
        assert_eq!(allocation_source_account("somewhere_else"), "3210");
    }

    #[test]
    fn utilizing_takes_from_the_balance_and_credits_bank_for_capital_purposes() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "res-1", "reserve_name": "Plant", "current_balance": "5000"
        })]);
        let answer = utilize_reserve(
            &graph,
            &scope(),
            "res-1",
            &json!({"amount": "1200", "purpose": "asset_purchase", "description": "buy a lathe"}),
        )
        .expect("utilized");
        assert_eq!(answer["reserve"]["current_balance"], json!("3800.000000"));
        let (_, write) = graph.statement(1);
        let lines = write["props"]["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["debit"], json!("1200.000000"));
        assert_eq!(lines[1]["account_code"], json!("1000"));
    }

    #[test]
    fn any_other_purpose_credits_dividend_payable() {
        assert_eq!(utilization_destination_account("working_capital"), "1000");
        assert_eq!(utilization_destination_account("dividend_support"), "2310");
        assert_eq!(utilization_destination_account(""), "2310");
    }

    #[test]
    fn utilizing_more_than_the_balance_is_refused_and_writes_nothing() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "res-1", "reserve_name": "Plant", "current_balance": "100"
        })]);
        let error = utilize_reserve(
            &graph,
            &scope(),
            "res-1",
            &json!({"amount": "100.01", "purpose": "working_capital"}),
        )
        .expect_err("refused");
        assert_eq!(error.to_string(), "Insufficient reserve balance");
        // only the read ran
        assert_eq!(graph.statements(), 1);
    }

    #[test]
    fn a_balance_below_the_minimum_is_still_utilizable() {
        // the service recorded a minimum and never enforced it
        let graph = FakeGraph::returning(vec![json!({
            "id": "res-1", "reserve_name": "Plant", "current_balance": "500",
            "minimum_balance": "1000"
        })]);
        let answer = utilize_reserve(
            &graph,
            &scope(),
            "res-1",
            &json!({"amount": "450", "purpose": "dividend_support"}),
        )
        .expect("utilized");
        assert_eq!(answer["reserve"]["current_balance"], json!("50.000000"));
    }
}
