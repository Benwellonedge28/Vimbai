#![forbid(unsafe_code)]
//! What a Book measures itself by, as rows rather than as code.
//!
//! A business does not only want revenue, cost and profit. It wants what its own trade cares about: spend on a line it
//! watches, a share of income from one kind of work, a count of what is still waiting to be posted. Writing each of those
//! as a function would mean a change to the engine every time a business asked for one, and an engine that names one
//! trade's concerns is an engine that fits that trade only.
//!
//! So a measure is a row. It says what it is called, what it is worked out from, and which of the Book's own accounts
//! count towards it. Adding a measure is adding a row; nothing here changes. The vocabulary is small on purpose, because
//! a measure is a view of what is already recorded and never a new way of reckoning:
//!
//! * `accounts` - what the Book's accounts of the named types, or the named numbers, moved over the window, or held at
//!   its end.
//! * `documents` - what is still owed on the Book's sales or purchase documents.
//! * `tax` - what the tax ledger says is owed.
//! * `entries` - how many entries in the window are in a given state.
//! * `difference` - one measure less one or more others.
//! * `percent` - one measure as a percentage of another.
//!
//! Every account type a row names is checked against the Book's own chart, so a Book that calls its accounts something
//! else still reads correctly and a row naming a type the Book does not keep is refused in the Book's own words. Nothing
//! here hard codes an account number, a currency, a tax or a trade.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, refused, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

use crate::cashbook;
use crate::ledger_book;
use crate::subsidiary_journals;
use crate::tax_ledger;

/// Where a Book's measures are kept: one record per measure, so a measure is a row and not a branch.
pub const MEASURE: RecordKind = RecordKind::new("BookMeasure", "OWNS_BOOK_MEASURE");

/// The kinds of thing a measure can be worked out from, in the words a row uses.
const SOURCES: [&str; 6] = [
    "accounts",
    "documents",
    "tax",
    "entries",
    "difference",
    "percent",
];

/// The units a measure can be stated in.
const UNITS: [&str; 3] = ["money", "percent", "count"];

/// The measures a Book starts with: the ones every business reads, whatever it trades in.
///
/// They are rows like any other, so a person can relabel one, add their own beside it, or take one away without the
/// engine changing. Nothing here names an account: revenue and costs are the Book's revenue and expense accounts, cash is
/// the money accounts it keeps, and what is owed each way is the documents it holds.
pub fn default_measures() -> Vec<Value> {
    vec![
        json!({
            "name": "revenue", "label": "Money in from trading", "unit": "money",
            "source": "accounts", "types": ["revenue"], "window": "period", "leads": true,
        }),
        json!({
            "name": "costs", "label": "Costs of trading", "unit": "money",
            "source": "accounts", "types": ["expense"], "window": "period", "leads": true,
        }),
        json!({
            "name": "profit", "label": "Profit", "unit": "money",
            "source": "difference", "of": "revenue", "minus": ["costs"], "leads": true,
        }),
        json!({
            "name": "margin", "label": "Profit as a share of trading", "unit": "percent",
            "source": "percent", "of": "profit", "over": "revenue", "leads": true,
        }),
        json!({
            "name": "cash", "label": "Cash held", "unit": "money",
            "source": "accounts", "types": ["asset"], "money_only": true, "window": "as_of",
            "leads": true,
        }),
        json!({
            "name": "owed_to_you", "label": "Owed to you", "unit": "money",
            "source": "documents", "documents": "sales", "leads": true,
        }),
        json!({
            "name": "owed_by_you", "label": "Owed by you", "unit": "money",
            "source": "documents", "documents": "purchases", "leads": true,
        }),
        json!({
            "name": "tax_set_aside", "label": "Tax set aside", "unit": "money",
            "source": "tax", "leads": false,
        }),
        json!({
            "name": "not_posted", "label": "Entries not yet posted", "unit": "count",
            "source": "entries", "status": "draft", "leads": false,
        }),
    ]
}

/// Give a Book the measures it starts with, once.
///
/// Safe to repeat: a measure the Book already holds by that name is left alone, so a person who relabels one, changes
/// what it counts or takes it away does not find it put back.
pub fn install_default_measures<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, MEASURE).list(scope, 5000)?;
    let names: Vec<String> = held.iter().map(|row| text(row, "name")).collect();
    let mut added = Vec::new();
    let mut already = Vec::new();
    for measure in default_measures() {
        let name = text(&measure, "name");
        if names.contains(&name) {
            already.push(name);
            continue;
        }
        Records::new(graph, MEASURE).create(scope, &measure)?;
        added.push(name);
    }
    Ok(json!({"added": added, "already there": already}))
}

/// The day the window ends, and the day it opens. A window that cannot be read is refused in words.
fn window(query: &Value) -> Result<(String, String), StoreError> {
    let to = match query.get("to").and_then(Value::as_str) {
        Some(given) if !given.trim().is_empty() => given.trim().to_string(),
        _ => {
            return Err(refused(
                "Say the day to report on, as to=2026-10-20. The day is asked for rather than assumed so that two \
                 readings can be compared.",
            ))
        }
    };
    if vimbai_common::dates::day_of(&to).is_none() {
        return Err(refused(&format!(
            "Say the day to report on as a day, 2026-10-20, rather than \"{to}\"."
        )));
    }
    let from = match query.get("from").and_then(Value::as_str) {
        Some(given) if !given.trim().is_empty() => {
            let given = given.trim().to_string();
            if vimbai_common::dates::day_of(&given).is_none() {
                return Err(refused(&format!(
                    "Say the day the window opens as a day, 2026-10-01, rather than \"{given}\"."
                )));
            }
            given
        }
        // the month so far, which is the window a person standing in a month means
        _ => format!("{}-01", &to[..7]),
    };
    if from > to {
        return Err(refused(&format!(
            "The window opens on {from} and closes on {to}, which is before it opens."
        )));
    }
    Ok((from, to))
}

/// What the Book's chart says each account is, so a row's words are checked against the Book and never assumed.
struct Chart {
    /// Account number to the type the Book gives it.
    types: BTreeMap<String, String>,
    /// Account number to its name, for saying which accounts a figure came from.
    names: BTreeMap<String, String>,
    /// The types this Book actually uses, in the Book's own words.
    used: Vec<String>,
    /// The numbers of the accounts that hold money.
    money: Vec<String>,
}

fn chart_of<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Chart, StoreError> {
    let mut chart = Chart {
        types: BTreeMap::new(),
        names: BTreeMap::new(),
        used: Vec::new(),
        money: Vec::new(),
    };
    for account in ledger_book::accounts(graph, scope)? {
        let number = field(&account, "account_number").to_string();
        if number.is_empty() {
            continue;
        }
        let kind = field(&account, "account_type").to_string();
        if !kind.is_empty() && !chart.used.contains(&kind) {
            chart.used.push(kind.clone());
        }
        chart
            .names
            .insert(number.clone(), field(&account, "account_name").to_string());
        chart.types.insert(number.clone(), kind);
    }
    let held = cashbook::list_accounts(graph, scope, &json!({"is_active": true}))?;
    for account in held.as_array().map(Vec::as_slice).unwrap_or(&[]) {
        let number = field(account, "account_number").to_string();
        if !number.is_empty() {
            chart.money.push(number);
        }
    }
    Ok(chart)
}

/// One measure worked out, or the reason it could not be.
#[allow(dead_code)]
enum Read {
    Money(Money),
    Count(u64),
    Problem(String),
}

/// The accounts a row names, as numbers.
fn numbers_of(row: &Value, chart: &Chart) -> Result<Vec<String>, StoreError> {
    let named: Vec<String> = row
        .get("numbers")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(Value::as_str)
        .map(|number| number.trim().to_string())
        .filter(|number| !number.is_empty())
        .collect();
    if !named.is_empty() {
        for number in &named {
            if !chart.types.contains_key(number) {
                return Err(refused(&format!(
                    "This Book has no account numbered {number}. A measure counts accounts the Book already keeps."
                )));
            }
        }
        return Ok(named);
    }
    let types: Vec<String> = row
        .get("types")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(Value::as_str)
        .map(|kind| kind.trim().to_string())
        .filter(|kind| !kind.is_empty())
        .collect();
    if types.is_empty() {
        return Err(refused(
            "Say which accounts a measure counts: a type such as expense, or the numbers of the accounts themselves.",
        ));
    }
    for kind in &types {
        if !chart.used.contains(kind) {
            return Err(refused(&format!(
                "This Book keeps no account of type {kind}. It keeps {}. A measure counts accounts by the words this \
                 Book uses.",
                if chart.used.is_empty() {
                    "none yet".to_string()
                } else {
                    chart.used.join(", ")
                }
            )));
        }
    }
    let money_only = row
        .get("money_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut numbers: Vec<String> = chart
        .types
        .iter()
        .filter(|(_, kind)| types.contains(kind))
        .map(|(number, _)| number.clone())
        .collect();
    if money_only {
        numbers.retain(|number| chart.money.contains(number));
    }
    Ok(numbers)
}

/// Work one row out. A row that cannot be read says so in words rather than answering nothing quietly.
fn read(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    from: &str,
    to: &str,
    chart: &Chart,
    row: &Value,
    worked: &BTreeMap<String, Money>,
) -> Result<Read, StoreError> {
    let source = text(row, "source");
    match source.as_str() {
        "accounts" => {
            let numbers = numbers_of(row, chart)?;
            let as_of = text(row, "window") == "as_of";
            let mut total = Money::zero();
            if as_of {
                let accounts = ledger_book::accounts(graph, scope)?;
                for account in &accounts {
                    let number = field(account, "account_number").to_string();
                    if !numbers.contains(&number) {
                        continue;
                    }
                    total = total.plus(&ledger_book::account_balance(
                        graph,
                        scope,
                        account,
                        Some(to),
                    )?);
                }
            } else {
                let moved = ledger_book::posted_activity(graph, scope, from, to)?;
                for number in &numbers {
                    if let Some(amount) = moved.get(number) {
                        total = total.plus(amount);
                    }
                }
            }
            Ok(Read::Money(total))
        }
        "documents" => {
            let which = text(row, "documents");
            let answer = match which.as_str() {
                "sales" => subsidiary_journals::receivables_ledger(
                    graph,
                    scope,
                    &json!({"as_of_date": to}),
                )?,
                "purchases" => {
                    subsidiary_journals::payables_ledger(graph, scope, &json!({"as_of_date": to}))?
                }
                other => {
                    return Err(refused(&format!(
                        "A measure over documents reads sales or purchases, not \"{other}\"."
                    )))
                }
            };
            Ok(Read::Money(
                Money::parse(field(&answer, "total_balance_due")).unwrap_or_else(|_| Money::zero()),
            ))
        }
        "tax" => {
            let answer = tax_ledger::tax_ledger(graph, scope, &json!({"from": from, "to": to}))?;
            Ok(Read::Money(
                Money::parse(field(&answer["totals"], "closing")).unwrap_or_else(|_| Money::zero()),
            ))
        }
        "entries" => {
            let wanted = text(row, "status");
            let entries = ledger_book::list_entries(
                graph,
                scope,
                &json!({"start_date": from, "end_date": to}),
            )?;
            let mut count = 0u64;
            for entry in entries.as_array().map(Vec::as_slice).unwrap_or(&[]) {
                let status = field(entry, "status");
                if status != "posted" && status != "draft" {
                    continue;
                }
                if wanted.is_empty() || status == wanted {
                    count += 1;
                }
            }
            Ok(Read::Count(count))
        }
        "difference" => {
            let from_measure = text(row, "of");
            let start = match worked.get(&from_measure) {
                Some(amount) => amount.clone(),
                None => {
                    return Ok(Read::Problem(format!(
                        "This measure is worked out from \"{from_measure}\", which this Book does not measure. It \
                         measures {}.",
                        measured(worked)
                    )))
                }
            };
            let mut total = start;
            for other in row
                .get("minus")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[])
                .iter()
                .filter_map(Value::as_str)
            {
                match worked.get(other) {
                    Some(amount) => total = total.minus(amount),
                    None => {
                        return Ok(Read::Problem(format!(
                            "This measure takes \"{other}\" away, and this Book does not measure it. It measures {}.",
                            measured(worked)
                        )))
                    }
                }
            }
            Ok(Read::Money(total))
        }
        "percent" => {
            let numerator = text(row, "of");
            let denominator = text(row, "over");
            let (Some(top), Some(bottom)) = (worked.get(&numerator), worked.get(&denominator))
            else {
                let missing = if worked.contains_key(&numerator) {
                    denominator
                } else {
                    numerator
                };
                return Ok(Read::Problem(format!(
                    "This measure is a share of \"{missing}\", which this Book does not measure. It measures {}.",
                    measured(worked)
                )));
            };
            // nothing is not a percentage of something: a share of no trading is no share at all
            if bottom.is_zero() {
                return Ok(Read::Money(Money::zero()));
            }
            Ok(Read::Money(top.ratio_pct(bottom, 2)))
        }
        other => Err(refused(&format!(
            "A measure is worked out from {}, not \"{other}\".",
            SOURCES.join(", ")
        ))),
    }
}

/// The measures this Book holds, named, for saying which one a row could not find.
fn measured(worked: &BTreeMap<String, Money>) -> String {
    if worked.is_empty() {
        return "nothing yet".to_string();
    }
    worked.keys().cloned().collect::<Vec<String>>().join(", ")
}

/// `GET /accounting/measures/`: what this Book measures itself by, worked out over a window.
///
/// A row that cannot be read answers with the reason beside it rather than hiding the rest: a Book with one mistaken
/// measure should still show its other figures.
pub fn list_measures<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let (from, to) = window(query)?;
    let chart = chart_of(graph, scope)?;
    let rows = Records::new(graph, MEASURE).list(scope, 5000)?;

    // a measure that is a difference or a share reads other measures, so the plain ones are worked out first
    let mut worked: BTreeMap<String, Money> = BTreeMap::new();
    let mut answers: Vec<Value> = Vec::new();
    for pass in 0..2 {
        for row in &rows {
            let source = text(row, "source");
            let derived = source == "difference" || source == "percent";
            if (pass == 0) == derived {
                continue;
            }
            let name = text(row, "name");
            let unit = text(row, "unit");
            let outcome = read(graph, scope, &from, &to, &chart, row, &worked);
            let answer = match outcome {
                Ok(Read::Money(amount)) => {
                    worked.insert(name.clone(), amount.clone());
                    json!({
                        "name": name,
                        "label": text(row, "label"),
                        "unit": if unit.is_empty() { "money".to_string() } else { unit },
                        "value": wire(&amount),
                        "leads": row.get("leads").and_then(Value::as_bool).unwrap_or(false),
                    })
                }
                Ok(Read::Count(count)) => json!({
                    "name": name,
                    "label": text(row, "label"),
                    "unit": "count",
                    "value": count,
                    "leads": row.get("leads").and_then(Value::as_bool).unwrap_or(false),
                }),
                Ok(Read::Problem(problem)) => json!({
                    "name": name,
                    "label": text(row, "label"),
                    "unit": unit,
                    "value": Value::Null,
                    "problem": problem,
                    "leads": row.get("leads").and_then(Value::as_bool).unwrap_or(false),
                }),
                Err(error) => {
                    return Err(StoreError::Invalid(format!("{name}: {error}")));
                }
            };
            answers.push(answer);
        }
    }
    Ok(json!({"from": from, "to": to, "measures": answers}))
}

/// `POST /accounting/measures/`: add a measure to this Book.
///
/// A measure is a row, so this checks the row against the Book rather than against a list of known measures: the accounts
/// it counts must exist here, and anything it reads from another measure must be a measure this Book has.
pub fn create_measure<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused(
            "Give the measure a name, as letters, digits and underscores: marketing_spend.",
        ));
    }
    if !name
        .chars()
        .all(|letter| letter.is_ascii_lowercase() || letter.is_ascii_digit() || letter == '_')
    {
        return Err(refused(&format!(
            "Write the name as lowercase letters, digits and underscores, as in marketing_spend, rather than \"{name}\"."
        )));
    }
    let label = text(payload, "label");
    if label.is_empty() {
        return Err(refused(
            "Say what the measure is called on a screen: a label a person reads, such as Marketing spend.",
        ));
    }
    let unit = {
        let given = text(payload, "unit");
        if given.is_empty() {
            "money".to_string()
        } else if UNITS.contains(&given.as_str()) {
            given
        } else {
            return Err(refused(&format!(
                "A measure is stated in {}, not \"{given}\".",
                UNITS.join(", ")
            )));
        }
    };
    let source = text(payload, "source");
    if !SOURCES.contains(&source.as_str()) {
        return Err(refused(&format!(
            "A measure is worked out from {}, not \"{source}\".",
            SOURCES.join(", ")
        )));
    }
    let chart = chart_of(graph, scope)?;
    let held = Records::new(graph, MEASURE).list(scope, 5000)?;
    let names: Vec<String> = held.iter().map(|row| text(row, "name")).collect();
    if names.contains(&name) {
        return Err(StoreError::Conflict(format!(
            "This Book already measures {name}. Give the new one a different name, or change the one it has."
        )));
    }
    match source.as_str() {
        "accounts" => {
            // the accounts are checked here, so a mistake is caught when the measure is written rather than when it is read
            numbers_of(payload, &chart)?;
        }
        "documents" => {
            let which = text(payload, "documents");
            if which != "sales" && which != "purchases" {
                return Err(refused(
                    "A measure over documents reads sales or purchases: say which.",
                ));
            }
        }
        "entries" => {
            let status = text(payload, "status");
            if !status.is_empty() && status != "draft" && status != "posted" {
                return Err(refused(&format!(
                    "Entries are counted as draft or posted, not \"{status}\"."
                )));
            }
        }
        "difference" | "percent" => {
            let reads: Vec<String> = if source == "percent" {
                vec![text(payload, "of"), text(payload, "over")]
            } else {
                let mut all = vec![text(payload, "of")];
                for other in payload
                    .get("minus")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(Value::as_str)
                {
                    all.push(other.to_string());
                }
                all
            };
            for other in reads {
                if other.is_empty() {
                    return Err(refused(if source == "percent" {
                        "Say which measure this is a share of, as of, and which it is a share over, as over."
                    } else {
                        "Say which measure this one starts from, as of, and which it takes away, as minus."
                    }));
                }
                if !names.contains(&other) {
                    return Err(refused(&format!(
                        "This Book does not measure \"{other}\", so {name} cannot be worked out from it. It measures \
                         {}.",
                        if names.is_empty() {
                            "nothing yet".to_string()
                        } else {
                            names.join(", ")
                        }
                    )));
                }
            }
        }
        _ => {}
    }

    let mut row = payload.clone();
    if let Value::Object(fields) = &mut row {
        fields.insert("unit".to_string(), json!(unit));
    }
    Records::new(graph, MEASURE).create(scope, &row)
}

/// `DELETE /accounting/measures/{name}`: stop measuring something.
pub fn delete_measure<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    name: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, MEASURE).list(scope, 5000)?;
    let row = held
        .iter()
        .find(|row| text(row, "name") == name)
        .cloned()
        .ok_or_else(|| refused(&format!("This Book does not measure \"{name}\".")))?;
    let id = text(&row, "id");
    Records::new(graph, MEASURE).delete(scope, &id)?;
    Ok(json!({"deleted": name}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::seed_opening_chart;
    use crate::ledger_book::create_entry;
    use crate::subsidiary_journals::{create_purchase, create_sale};
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    /// A business Book that has traded in October: one taxed sale on the 5th and one taxed purchase on the 9th.
    fn traded() -> MemoryGraph {
        let graph = MemoryGraph::new();
        seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        install_default_measures(&graph, &scope()).expect("measures");
        create_sale(
            &graph,
            &scope(),
            &json!({"invoice_number": "INV-1", "entry_date": "2026-10-05",
                    "total_amount": "1150.00", "tax_amount": "150.00"}),
        )
        .expect("a taxed sale");
        create_purchase(
            &graph,
            &scope(),
            &json!({"bill_number": "BILL-1", "entry_date": "2026-10-09",
                    "total_amount": "460.00", "tax_amount": "60.00"}),
        )
        .expect("a taxed purchase");
        graph
    }

    fn measure<'a>(answer: &'a Value, name: &str) -> &'a Value {
        answer["measures"]
            .as_array()
            .expect("measures")
            .iter()
            .find(|row| row["name"] == json!(name))
            .unwrap_or_else(|| panic!("no measure {name} in {answer}"))
    }

    #[test]
    fn a_book_starts_with_the_measures_every_business_reads() {
        let graph = MemoryGraph::new();
        seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        let installed = install_default_measures(&graph, &scope()).expect("installed");
        assert_eq!(installed["added"].as_array().map(Vec::len), Some(9));

        // installing again adds nothing: a person who changed a measure does not find it put back
        let again = install_default_measures(&graph, &scope()).expect("installed");
        assert_eq!(again["added"].as_array().map(Vec::len), Some(0));
        assert_eq!(again["already there"].as_array().map(Vec::len), Some(9));
    }

    #[test]
    fn revenue_costs_and_profit_read_the_books_own_accounts() {
        let graph = traded();
        let answer = list_measures(&graph, &scope(), &json!({"to": "2026-10-31"})).expect("read");
        // revenue is what the revenue accounts took up and costs what the expense accounts did, as the statements read
        // them; profit is the one less the other, and it is a row rather than a second way of adding the ledger up
        assert_eq!(measure(&answer, "revenue")["value"], json!("1150.000000"));
        // buying stock is not a cost until the stock is sold: it went to the inventory account, which is an asset, so
        // what the expense accounts took up is the tax the sale carried and nothing else
        assert_eq!(measure(&answer, "costs")["value"], json!("150.000000"));
        assert_eq!(measure(&answer, "profit")["value"], json!("1000.000000"));
        // and a share is a share of trading, not of nothing
        assert_eq!(measure(&answer, "margin")["value"], json!("86.960000"));
    }

    #[test]
    fn what_is_owed_each_way_is_read_from_the_documents_the_book_holds() {
        let graph = traded();
        let answer = list_measures(&graph, &scope(), &json!({"to": "2026-10-31"})).expect("read");
        assert_eq!(
            measure(&answer, "owed_to_you")["value"],
            json!("1150.000000")
        );
        assert_eq!(
            measure(&answer, "owed_by_you")["value"],
            json!("460.000000")
        );
    }

    #[test]
    fn a_measure_a_person_adds_is_a_row_and_counts_only_the_accounts_it_names() {
        let graph = traded();
        create_measure(
            &graph,
            &scope(),
            &json!({"name": "premises", "label": "Premises", "source": "accounts",
                    "types": ["expense"], "numbers": ["6100"]}),
        )
        .expect("a measure");

        // a purchase touches the purchases account, not rent, so a measure of rent is nothing yet
        let answer = list_measures(&graph, &scope(), &json!({"to": "2026-10-31"})).expect("read");
        assert_eq!(measure(&answer, "premises")["value"], json!("0.000000"));

        // and once rent is paid it counts, without anything in the engine changing. It is posted, because a measure reads
        // what the ledger holds: a draft moves nothing, which is the next test's subject.
        let rent = create_entry(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-10-20", "reference": "RENT-1", "description": "October rent",
                    "lines": [
                        {"account_number": "6100", "debit": "300.00", "credit": "0.00"},
                        {"account_number": "1000", "debit": "0.00", "credit": "300.00"}
                    ]}),
        )
        .expect("rent");
        ledger_book::post_draft(&graph, &scope(), &text(&rent, "id")).expect("posted");
        let answer = list_measures(&graph, &scope(), &json!({"to": "2026-10-31"})).expect("read");
        assert_eq!(measure(&answer, "premises")["value"], json!("300.000000"));
        // and costs rose with it, because costs are every expense account, which is a row and not a list in code
        assert_eq!(measure(&answer, "costs")["value"], json!("450.000000"));
    }

    #[test]
    fn a_measure_of_cash_reads_the_money_accounts_and_not_every_asset() {
        let graph = traded();
        // every asset, which is the debtor's invoice as well as any money in hand
        create_measure(
            &graph,
            &scope(),
            &json!({"name": "assets", "label": "Everything the business holds", "source": "accounts",
                    "types": ["asset"], "window": "as_of"}),
        )
        .expect("a measure");
        let answer = list_measures(&graph, &scope(), &json!({"to": "2026-10-31"})).expect("read");
        // the invoice is an asset, but it is not cash: money_only is what separates the two, and it is a field on a row
        // the invoice and the stock on hand are both assets, and neither of them is money
        assert_eq!(measure(&answer, "assets")["value"], json!("1550.000000"));
        assert_eq!(measure(&answer, "cash")["value"], json!("0.000000"));
    }

    #[test]
    fn a_measure_naming_a_type_this_book_does_not_keep_is_refused_in_the_books_own_words() {
        let graph = traded();
        let refused = create_measure(
            &graph,
            &scope(),
            &json!({"name": "livestock", "label": "Livestock", "source": "accounts",
                    "types": ["livestock"]}),
        )
        .expect_err("a type the Book does not keep");
        let said = refused.to_string();
        assert!(
            said.contains("keeps no account of type livestock"),
            "{said}"
        );
        assert!(said.contains("expense"), "{said}");
    }

    #[test]
    fn an_unknown_source_is_refused_naming_the_ones_that_exist() {
        let graph = traded();
        let refused = create_measure(
            &graph,
            &scope(),
            &json!({"name": "magic", "label": "Magic", "source": "guesswork"}),
        )
        .expect_err("an unknown source");
        let said = refused.to_string();
        assert!(said.contains("guesswork"), "{said}");
        assert!(said.contains("difference"), "{said}");
    }

    #[test]
    fn a_share_of_a_measure_this_book_does_not_have_is_refused_naming_the_ones_it_has() {
        let graph = traded();
        let refused = create_measure(
            &graph,
            &scope(),
            &json!({"name": "share", "label": "Share", "source": "percent",
                    "of": "profit", "over": "turnover"}),
        )
        .expect_err("no turnover");
        let said = refused.to_string();
        assert!(said.contains("turnover"), "{said}");
        assert!(said.contains("revenue"), "{said}");
    }

    #[test]
    fn a_measure_added_twice_is_refused_rather_than_shadowing_the_first() {
        let graph = traded();
        let refused = create_measure(
            &graph,
            &scope(),
            &json!({"name": "revenue", "label": "Money in", "source": "accounts", "types": ["revenue"]}),
        )
        .expect_err("already measured");
        assert!(
            refused.to_string().contains("already measures revenue"),
            "{refused}"
        );
    }

    #[test]
    fn not_posted_counts_the_entries_the_statements_leave_out() {
        let graph = traded();
        let before = list_measures(&graph, &scope(), &json!({"to": "2026-10-31"})).expect("read");
        assert_eq!(measure(&before, "not_posted")["value"], json!(0));

        create_entry(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-10-21", "reference": "DRAFT-1", "status": "draft",
            "description": "a bill not yet posted",
            "lines": [
                {"account_number": "6100", "debit": "50.00", "credit": "0.00"},
                {"account_number": "2000", "debit": "0.00", "credit": "50.00"}
            ]}),
        )
        .expect("a draft");
        let after = list_measures(&graph, &scope(), &json!({"to": "2026-10-31"})).expect("read");
        assert_eq!(measure(&after, "not_posted")["value"], json!(1));
    }

    #[test]
    fn a_window_that_cannot_be_read_is_refused_in_words() {
        let graph = traded();
        let refused = list_measures(&graph, &scope(), &json!({})).expect_err("no day");
        assert!(
            refused.to_string().contains("as to=2026-10-20"),
            "{refused}"
        );
        let refused =
            list_measures(&graph, &scope(), &json!({"to": "31 October"})).expect_err("not a day");
        assert!(refused.to_string().contains("31 October"), "{refused}");
    }

    #[test]
    fn a_measure_can_be_taken_away() {
        let graph = traded();
        delete_measure(&graph, &scope(), "margin").expect("deleted");
        let answer = list_measures(&graph, &scope(), &json!({"to": "2026-10-31"})).expect("read");
        assert!(!answer["measures"]
            .as_array()
            .expect("measures")
            .iter()
            .any(|row| row["name"] == json!("margin")));
        let refused = delete_measure(&graph, &scope(), "margin").expect_err("gone");
        assert!(
            refused.to_string().contains("does not measure"),
            "{refused}"
        );
    }
}
