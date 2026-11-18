#![forbid(unsafe_code)]
//! A Book's spending plan, on the store.
//!
//! The budget screens used to keep their plan on the phone that typed it, so a Book shared with a second person arrived
//! with no plan at all, and a plan typed on a lost phone was gone. A plan belongs to the Book: it is kept here, by the
//! engine, and every device sharing the Book reads the same one.
//!
//! The figures are `Money`, held as exact decimal strings, so a plan of 1,234,567.89 does not drift as it is compared
//! against what was actually spent.
//!
//! What is refused, and why it matters:
//!
//! * A budget with no name, or ending before it starts, is not a plan for anything.
//! * A budget with no lines plans nothing: it would read as "planned nothing, spent nothing" on every screen.
//! * A line of nothing is refused. A zero line and a missing line look the same on a variance report, so accepting one
//!   would quietly add a row that says nothing.
//! * Two lines for the same account are refused, naming the account. The variance adds the lines up, so a second line for
//!   an account would count its plan twice while the ledger counts its spending once.
//!
//! The variance itself is arithmetic over figures it is given: what the plan says for an account and what the posted
//! ledger says was spent on it. It never reads the ledger itself, so there is exactly one place that decides what "spent"
//! means, and that place is the accounting crate.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount, wire};
use vimbai_store::read::text;
use vimbai_store::{BookScope, Graph, Records, StoreError, BOOK_BUDGET};

/// A line of a plan: what one account is meant to take up, and what it is for.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub category: String,
    pub account_number: String,
    pub budgeted: Money,
}

fn lines_of(record: &Value) -> Result<Vec<Line>, StoreError> {
    let raw = record.get("lines").and_then(Value::as_str).unwrap_or("[]");
    let rows: Vec<Value> = serde_json::from_str(raw).unwrap_or_default();
    let mut lines = Vec::with_capacity(rows.len());
    for row in rows {
        lines.push(Line {
            category: text(&row, "category"),
            account_number: text(&row, "account_number"),
            budgeted: amount(&row, "budgeted_amount")?,
        });
    }
    Ok(lines)
}

fn published(record: &Value) -> Result<Value, StoreError> {
    let lines = lines_of(record)?;
    let mut total = Money::zero();
    let rows: Vec<Value> = lines
        .iter()
        .map(|line| {
            total = total.plus(&line.budgeted);
            json!({
                "category": line.category,
                "account_number": line.account_number,
                "budgeted_amount": wire(&line.budgeted),
            })
        })
        .collect();
    Ok(json!({
        "id": text(record, "id"),
        "name": text(record, "name"),
        "start_date": text(record, "start_date"),
        "end_date": text(record, "end_date"),
        "currency": text(record, "currency"),
        "description": text(record, "description"),
        "status": text(record, "status"),
        "lines": rows,
        "line_count": rows.len(),
        "total_budgeted": wire(&total),
        "created_at": text(record, "created_at"),
    }))
}

/// The plan as it was sent, checked and turned into the record to store.
fn checked(payload: &Value) -> Result<(Value, Vec<Line>), StoreError> {
    let name = text(payload, "name");
    if name.trim().is_empty() {
        return Err(StoreError::Invalid("name is required".into()));
    }
    let start_date = text(payload, "start_date");
    let end_date = text(payload, "end_date");
    if start_date.trim().is_empty() || end_date.trim().is_empty() {
        return Err(StoreError::Invalid(
            "start_date and end_date are required".into(),
        ));
    }
    if end_date < start_date {
        return Err(StoreError::Invalid(
            "the plan cannot end before it starts".into(),
        ));
    }

    let rows = payload
        .get("lines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if rows.is_empty() {
        return Err(StoreError::Invalid(
            "a plan needs at least one line: with no lines it plans nothing".into(),
        ));
    }

    let mut lines: Vec<Line> = Vec::with_capacity(rows.len());
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for (index, row) in rows.iter().enumerate() {
        let account_number = text(row, "account_number").trim().to_string();
        if account_number.is_empty() {
            return Err(StoreError::Invalid(format!(
                "line {} names no account",
                index + 1
            )));
        }
        if let Some(earlier) = seen.insert(account_number.clone(), index) {
            return Err(StoreError::Invalid(format!(
                "account {account_number} is planned on line {} and again on line {}: one line per account, or its \
                 plan would be counted twice",
                earlier + 1,
                index + 1
            )));
        }
        let budgeted = amount(row, "budgeted_amount").map_err(|error| {
            StoreError::Invalid(format!("line {} ({account_number}): {error}", index + 1))
        })?;
        if budgeted.is_zero() {
            return Err(StoreError::Invalid(format!(
                "line {} ({account_number}) plans nothing: a zero line reads the same as no line",
                index + 1
            )));
        }
        if budgeted.is_negative() {
            return Err(StoreError::Invalid(format!(
                "line {} ({account_number}) is negative: a plan of minus money is not a plan",
                index + 1
            )));
        }
        lines.push(Line {
            category: text(row, "category"),
            account_number,
            budgeted,
        });
    }

    let stored_lines: Vec<Value> = lines
        .iter()
        .map(|line| {
            json!({
                "category": line.category,
                "account_number": line.account_number,
                "budgeted_amount": line.budgeted.to_fixed(6),
            })
        })
        .collect();

    let currency = {
        let given = text(payload, "currency");
        if given.trim().is_empty() {
            "USD".to_string()
        } else {
            given
        }
    };

    let record = json!({
        "name": name.trim(),
        "start_date": start_date,
        "end_date": end_date,
        "currency": currency,
        "description": text(payload, "description"),
        "status": "draft",
        "lines": serde_json::to_string(&stored_lines).unwrap_or_else(|_| "[]".to_owned()),
    });
    Ok((record, lines))
}

/// `/budgets`: keep a plan.
pub fn create<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let (record, _) = checked(payload)?;
    let stored = Records::new(graph, BOOK_BUDGET).create(scope, &record)?;
    published(&stored)
}

/// `/budgets`: every plan on the Book, newest first.
pub fn list_budgets<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    let mut rows = Records::new(graph, BOOK_BUDGET).list(scope, 500)?;
    rows.sort_by_key(|row| std::cmp::Reverse(text(row, "start_date")));
    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        out.push(published(row)?);
    }
    Ok(Value::Array(out))
}

pub fn get_budget<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    budget_id: &str,
) -> Result<Value, StoreError> {
    published(&Records::new(graph, BOOK_BUDGET).get(scope, budget_id)?)
}

/// `/budgets/{id}`: a plan removed. Its lines go with it, because they are part of the record.
pub fn remove_budget<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    budget_id: &str,
) -> Result<Value, StoreError> {
    let existing = Records::new(graph, BOOK_BUDGET).get(scope, budget_id)?;
    Records::new(graph, BOOK_BUDGET).delete(scope, budget_id)?;
    Ok(json!({
        "id": text(&existing, "id"),
        "name": text(&existing, "name"),
        "removed": true,
    }))
}

/// One account's line against what was actually spent on it.
fn line_variance(line: &Line, spent: &Money) -> Value {
    let difference = line.budgeted.minus(spent);
    let state = if difference.is_negative() {
        "over"
    } else if difference.is_zero() {
        "on"
    } else {
        "under"
    };
    json!({
        "category": line.category,
        "account_number": line.account_number,
        "budgeted_amount": wire(&line.budgeted),
        "against_plan_amount": wire(spent),
        // what is left of the plan: negative means the account has gone past it
        "remaining_amount": wire(&difference),
        "state": state,
    })
}

/// `/budgets/{id}/variance`: the plan against what was spent, account by account.
///
/// `spent` is what the ledger says each account has taken up, already signed so that a positive figure is spending. An
/// account the ledger says nothing about has spent nothing, which is a real answer rather than a missing one.
pub fn variance<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    budget_id: &str,
    spent: &BTreeMap<String, Money>,
) -> Result<Value, StoreError> {
    let record = Records::new(graph, BOOK_BUDGET).get(scope, budget_id)?;
    let lines = lines_of(&record)?;

    let nothing = Money::zero();
    let mut rows = Vec::with_capacity(lines.len());
    let mut total_budgeted = Money::zero();
    let mut total_against_plan = Money::zero();
    let mut over: Vec<Value> = Vec::new();
    let mut worst: Option<(String, Money)> = None;

    for line in &lines {
        let spent_here = spent
            .get(&line.account_number)
            .cloned()
            .unwrap_or(nothing.clone());
        total_budgeted = total_budgeted.plus(&line.budgeted);
        total_against_plan = total_against_plan.plus(&spent_here);
        let row = line_variance(line, &spent_here);
        if row["state"] == json!("over") {
            let past = spent_here.minus(&line.budgeted);
            over.push(json!({
                "account_number": line.account_number,
                "category": line.category,
                "past_by": wire(&past),
            }));
            let worse = match &worst {
                None => true,
                Some((_, amount)) => past > *amount,
            };
            if worse {
                worst = Some((line.account_number.clone(), past));
            }
        }
        rows.push(row);
    }

    // the biggest overspend first, so the one to look at is the one at the top
    rows.sort_by(|a, b| {
        let key = |row: &Value| {
            let remaining = Money::parse(row["remaining_amount"].as_str().unwrap_or("0"))
                .unwrap_or_else(|_| Money::zero());
            remaining
        };
        key(a).partial_cmp_amount(&key(b))
    });

    let remaining = total_budgeted.minus(&total_against_plan);
    let verdict = if over.is_empty() { "within" } else { "over" };
    Ok(json!({
        "id": text(&record, "id"),
        "name": text(&record, "name"),
        "start_date": text(&record, "start_date"),
        "end_date": text(&record, "end_date"),
        "currency": text(&record, "currency"),
        "total_budgeted": wire(&total_budgeted),
        "total_against_plan": wire(&total_against_plan),
        "remaining_amount": wire(&remaining),
        "verdict": verdict,
        "accounts_over": over.len(),
        "worst_account": worst.map(|(account, past)| json!({
            "account_number": account,
            "past_by": wire(&past),
        })),
        "over": over,
        "lines": rows,
    }))
}

/// Orders two amounts, smallest first, for the report's own sorting.
trait OrderedAmount {
    fn partial_cmp_amount(&self, other: &Money) -> std::cmp::Ordering;
}

impl OrderedAmount for Money {
    fn partial_cmp_amount(&self, other: &Money) -> std::cmp::Ordering {
        if self < other {
            std::cmp::Ordering::Less
        } else if self > other {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("book-1", "user-1")
    }

    fn plan(lines: Value) -> Value {
        json!({
            "name": "2027 plan",
            "start_date": "2027-01-01",
            "end_date": "2027-12-31",
            "currency": "USD",
            "lines": lines,
        })
    }

    /// The figures leave as six-place decimal strings, so a test compares the amount it means rather than the shape of
    /// the string carrying it.
    fn amount_is(value: &Value, expected: &str) -> bool {
        let published = value.as_str().unwrap_or("<not an amount>");
        match (Money::parse(published), Money::parse(expected)) {
            (Ok(left), Ok(right)) => left == right,
            _ => false,
        }
    }

    #[test]
    fn a_plan_is_kept_with_its_lines_and_total() {
        let graph = MemoryGraph::new();
        let saved = create(
            &graph,
            &scope(),
            &plan(json!([
                {"category": "cost of sales", "account_number": "5000", "budgeted_amount": "1200.00"},
                {"category": "rent", "account_number": "6100", "budgeted_amount": "800.50"},
            ])),
        )
        .expect("saved");
        assert_eq!(saved["line_count"], json!(2));
        assert!(amount_is(&saved["total_budgeted"], "2000.50"));
        assert!(amount_is(&saved["lines"][1]["budgeted_amount"], "800.50"));
        assert_eq!(saved["status"], json!("draft"));
    }

    #[test]
    fn a_plan_with_no_lines_is_refused() {
        let graph = MemoryGraph::new();
        let error = create(&graph, &scope(), &plan(json!([]))).expect_err("refused");
        assert!(error.to_string().contains("at least one line"), "{error}");
    }

    #[test]
    fn a_line_of_nothing_is_refused() {
        let graph = MemoryGraph::new();
        let error = create(
            &graph,
            &scope(),
            &plan(json!([{"account_number": "5000", "budgeted_amount": "0.00"}])),
        )
        .expect_err("refused");
        assert!(error.to_string().contains("plans nothing"), "{error}");
    }

    #[test]
    fn two_lines_for_one_account_are_refused_and_the_account_is_named() {
        let graph = MemoryGraph::new();
        let error = create(
            &graph,
            &scope(),
            &plan(json!([
                {"account_number": "5000", "budgeted_amount": "10.00"},
                {"account_number": "5000", "budgeted_amount": "20.00"},
            ])),
        )
        .expect_err("refused");
        assert!(error.to_string().contains("5000"), "{error}");
        assert!(error.to_string().contains("counted twice"), "{error}");
    }

    #[test]
    fn a_plan_cannot_end_before_it_starts() {
        let graph = MemoryGraph::new();
        let error = create(
            &graph,
            &scope(),
            &json!({
                "name": "backwards",
                "start_date": "2027-12-31",
                "end_date": "2027-01-01",
                "lines": [{"account_number": "5000", "budgeted_amount": "1.00"}],
            }),
        )
        .expect_err("refused");
        assert!(
            error.to_string().contains("end before it starts"),
            "{error}"
        );
    }

    #[test]
    fn an_amount_past_double_precision_keeps_every_digit() {
        let graph = MemoryGraph::new();
        let saved = create(
            &graph,
            &scope(),
            &plan(json!([
                {"account_number": "5000", "budgeted_amount": "123456789012345678901.55"}
            ])),
        )
        .expect("saved");
        assert!(amount_is(
            &saved["total_budgeted"],
            "123456789012345678901.55"
        ));
    }

    #[test]
    fn a_plan_is_read_back_by_id_and_listed_newest_first() {
        let graph = MemoryGraph::new();
        let first = create(
            &graph,
            &scope(),
            &json!({
                "name": "2027 plan",
                "start_date": "2027-01-01",
                "end_date": "2027-12-31",
                "lines": [{"account_number": "6100", "budgeted_amount": "800.00"}],
            }),
        )
        .expect("saved");
        create(
            &graph,
            &scope(),
            &json!({
                "name": "2028 plan",
                "start_date": "2028-01-01",
                "end_date": "2028-12-31",
                "lines": [{"account_number": "6100", "budgeted_amount": "900.00"}],
            }),
        )
        .expect("saved");

        let id = first["id"].as_str().expect("id");
        let read_back = get_budget(&graph, &scope(), id).expect("read");
        assert_eq!(read_back["name"], json!("2027 plan"));

        let listed = list_budgets(&graph, &scope()).expect("listed");
        let rows = listed.as_array().expect("rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"], json!("2028 plan"));
    }

    #[test]
    fn a_removed_plan_is_gone_from_the_list() {
        let graph = MemoryGraph::new();
        let saved = create(
            &graph,
            &scope(),
            &plan(json!([{"account_number": "6100", "budgeted_amount": "800.00"}])),
        )
        .expect("saved");
        let id = saved["id"].as_str().expect("id");
        let removed = remove_budget(&graph, &scope(), id).expect("removed");
        assert_eq!(removed["removed"], json!(true));
        let listed = list_budgets(&graph, &scope()).expect("listed");
        assert_eq!(listed.as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn the_variance_says_what_is_left_and_which_accounts_are_past_it() {
        let graph = MemoryGraph::new();
        let saved = create(
            &graph,
            &scope(),
            &plan(json!([
                {"category": "rent", "account_number": "6100", "budgeted_amount": "800.00"},
                {"category": "fuel", "account_number": "6200", "budgeted_amount": "200.00"},
                {"category": "wages", "account_number": "7000", "budgeted_amount": "500.00"},
            ])),
        )
        .expect("saved");
        let id = saved["id"].as_str().expect("id");

        let mut spent = BTreeMap::new();
        spent.insert("6100".to_string(), Money::parse("800.00").expect("rent"));
        spent.insert("6200".to_string(), Money::parse("350.00").expect("fuel"));
        // 7000 is not in the map at all: the ledger says nothing, which means nothing spent

        let report = variance(&graph, &scope(), id, &spent).expect("report");
        assert!(amount_is(&report["total_budgeted"], "1500.00"));
        assert!(amount_is(&report["total_against_plan"], "1150.00"));
        assert!(amount_is(&report["remaining_amount"], "350.00"));
        assert_eq!(report["verdict"], json!("over"));
        assert_eq!(report["accounts_over"], json!(1));
        assert_eq!(report["worst_account"]["account_number"], json!("6200"));
        assert!(amount_is(&report["worst_account"]["past_by"], "150.00"));

        // the account that has gone past its plan is at the top, the untouched one at the bottom
        let rows = report["lines"].as_array().expect("rows");
        assert_eq!(rows[0]["account_number"], json!("6200"));
        assert_eq!(rows[0]["state"], json!("over"));
        assert!(amount_is(&rows[0]["remaining_amount"], "-150.00"));
        assert_eq!(rows[2]["account_number"], json!("7000"));
        assert!(amount_is(&rows[2]["against_plan_amount"], "0.00"));
        assert_eq!(rows[2]["state"], json!("under"));
    }

    #[test]
    fn a_plan_nothing_has_been_spent_against_says_within() {
        let graph = MemoryGraph::new();
        let saved = create(
            &graph,
            &scope(),
            &plan(json!([{"account_number": "6100", "budgeted_amount": "800.00"}])),
        )
        .expect("saved");
        let id = saved["id"].as_str().expect("id");
        let report = variance(&graph, &scope(), id, &BTreeMap::new()).expect("report");
        assert_eq!(report["verdict"], json!("within"));
        assert!(amount_is(&report["total_against_plan"], "0.00"));
        assert!(amount_is(&report["remaining_amount"], "800.00"));
        assert_eq!(report["accounts_over"], json!(0));
        assert_eq!(report["worst_account"], Value::Null);
    }

    #[test]
    fn spending_exactly_the_plan_is_on_it_rather_than_under_or_over() {
        let graph = MemoryGraph::new();
        let saved = create(
            &graph,
            &scope(),
            &plan(json!([{"account_number": "6100", "budgeted_amount": "800.00"}])),
        )
        .expect("saved");
        let id = saved["id"].as_str().expect("id");
        let mut spent = BTreeMap::new();
        spent.insert("6100".to_string(), Money::parse("800.00").expect("spent"));
        let report = variance(&graph, &scope(), id, &spent).expect("report");
        assert_eq!(report["verdict"], json!("within"));
        assert_eq!(report["lines"][0]["state"], json!("on"));
        assert!(amount_is(&report["lines"][0]["remaining_amount"], "0.00"));
    }
}
