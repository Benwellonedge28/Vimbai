//! The depreciation journal: the wear on what the business owns, reaching the ledger.
//!
//! Everything a business keeps rather than sells - a vehicle, a fridge, a laptop, a machine - loses value as it is used, and
//! that loss is a cost of trading. It is a cost with no money behind it: nothing leaves the bank in the month the fridge
//! ages. So it cannot go through the cash book, and it is not a bill anybody will send.
//!
//! The register already exists and already does the arithmetic: `crates/domains/assets` holds the asset, its cost, its life
//! and its salvage value, and records one row per month per asset with the charge and the accumulated figure. What it never
//! did was reach the books, so a Book's profit included no wear at all and its assets were carried at what they cost on the
//! day they were bought. This journal is the missing half: it takes the rows the register recorded and posts them.
//!
//! Kept here rather than computed again, deliberately:
//!
//! * the charge is read from the register's own row. Computing it a second time in the ledger would give two arithmetics
//!   for one figure, and the day they disagree nobody can say which one the books meant;
//! * a month is posted once. The row carries the id of the entry that posted it, so a second run of the same month finds
//!   nothing left to post rather than charging the wear twice;
//! * the charge goes to a depreciation expense account and the same amount to accumulated depreciation, which is a
//!   contra-asset: the thing stays in the books at what it cost, and what has been used up is kept beside it, so a reader
//!   can see both the cost and the wear;
//! * a closed month is not written into. The charge belongs in the month that is still open.
//!
//! Vimbai does not move money: depreciation is the one journal where there is no money to move.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::read::{field, money, refused, text};
use vimbai_store::{BookScope, Graph, Records, StoreError, DEPRECIATION_ENTRY, FIXED_ASSET};

use crate::ledger_book;
use crate::periods;
use crate::subsidiary_journals::post;

/// The account the charge is a cost of, and the account that holds the wear.
///
/// A Book numbers its own chart, so neither is guessed from a number. A caller may name them; otherwise the chart's own
/// names are read, and a Book that has neither is refused rather than posting a cost to nothing.
struct Accounts {
    expense: String,
    accumulated: String,
}

/// What the chart calls an account.
///
/// The chart's own records name an account under `name`; a caller that read it through the ledger's own view may have it as
/// `account_name`. Both are the same fact, so both are read rather than one of them silently coming back empty.
fn name_of(account: &Value) -> String {
    let named = field(account, "account_name");
    if named.trim().is_empty() {
        field(account, "name").to_string()
    } else {
        named.to_string()
    }
}

/// An account named in the payload, or the one the chart calls this.
fn account_named<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
    key: &str,
    called: &str,
    kind: &str,
) -> Result<Option<String>, StoreError> {
    let named = text(payload, key);
    let accounts = ledger_book::accounts(graph, scope)?;
    if !named.trim().is_empty() {
        let found = accounts
            .iter()
            .any(|account| field(account, "account_number") == named.trim());
        if !found {
            return Err(refused(&format!(
                "Account {named} is not in this Book's chart, so nothing can be posted to it."
            )));
        }
        return Ok(Some(named.trim().to_string()));
    }
    // both sides say "depreciation" in their name, so the type is what tells the cost from the wear: the charge is an
    // expense and what has been used up is a contra-asset
    Ok(accounts
        .iter()
        .find(|account| {
            field(account, "account_type") == kind
                && name_of(account)
                    .to_lowercase()
                    .contains(&called.to_lowercase())
        })
        .map(|account| field(account, "account_number").to_string()))
}

fn accounts_for<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Accounts, StoreError> {
    let expense = account_named(
        graph,
        scope,
        payload,
        "expense_account",
        "depreciation",
        "expense",
    )?
    .ok_or_else(
        || {
            refused(
                "This Book has no account for depreciation, so say which account the charge is a cost of.",
            )
        },
    )?;
    let accumulated = account_named(
        graph,
        scope,
        payload,
        "accumulated_account",
        "depreciation",
        "asset",
    )?
    .ok_or_else(|| {
        refused(
            "This Book has no account for accumulated depreciation, so say which account holds how much of its assets have been used up.",
        )
    })?;
    if expense == accumulated {
        return Err(refused(
            "What has been used up is kept apart from what it cost the business, so the charge and the wear cannot be the same account.",
        ));
    }
    Ok(Accounts {
        expense,
        accumulated,
    })
}

/// What the register calls its assets, so a charge can be spoken about as the thing it belongs to rather than by an id
/// nobody recognises.
///
/// The register owns the name; this only reads it, so there is one place a thing is named.
fn asset_names<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
) -> Result<Vec<(String, String)>, StoreError> {
    Ok(Records::new(graph, FIXED_ASSET)
        .list(scope, 1000)?
        .iter()
        .map(|asset| {
            (
                field(asset, "id").to_string(),
                field(asset, "asset_name").to_string(),
            )
        })
        .collect())
}

/// The same row with the name of the thing it is about.
fn with_asset_name(mut row: Value, names: &[(String, String)]) -> Value {
    let id = field(&row, "asset_id").to_string();
    let named = names
        .iter()
        .find(|(asset, _)| *asset == id)
        .map(|(_, name)| name.clone())
        .unwrap_or_default();
    if let Some(object) = row.as_object_mut() {
        object.insert("asset_name".to_string(), json!(named));
    }
    row
}

/// Every depreciation row the register recorded for a period, newest row last.
fn rows_for<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    period: &str,
) -> Result<Vec<Value>, StoreError> {
    Ok(Records::new(graph, DEPRECIATION_ENTRY)
        .list(scope, 5000)?
        .into_iter()
        .filter(|row| field(row, "period") == period)
        .collect())
}

/// Post one register row, unless it has already been posted.
fn post_row<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    row: &Value,
    accounts: &Accounts,
    entry_date: &str,
    created_at: &str,
) -> Result<Option<Value>, StoreError> {
    // a row that already carries an entry is posted: posting it again would charge the same month's wear twice
    if !field(row, "journal_entry_id").trim().is_empty() {
        return Ok(None);
    }
    let row_id = field(row, "id").to_string();
    let asset_id = field(row, "asset_id").to_string();
    let period = field(row, "period").to_string();
    let amount = money(row, "depreciation_amount")?;
    if amount.is_zero() || amount.is_negative() {
        return Err(refused(
            "That month's depreciation is nothing, so there is no charge to post.",
        ));
    }
    let reference = format!("DEP-{asset_id}-{period}");
    let entry_id = post(
        graph,
        scope,
        &json!({"entry_date": entry_date, "created_at": created_at}),
        format!("Depreciation for {period}"),
        &reference,
        "Depreciation",
        &accounts.expense,
        &accounts.accumulated,
        &amount,
        &format!("{period} wear"),
    )?;
    // the row keeps the entry that posted it, which is what stops a second run posting the same month again
    let stamped = Records::new(graph, DEPRECIATION_ENTRY).update(
        scope,
        &row_id,
        &json!({"journal_entry_id": entry_id}),
    )?;
    Ok(Some(stamped))
}

/// Post a month's depreciation from the register.
///
/// The month is stated as `2026-11`. Every row the register recorded for that month that no entry has posted yet is
/// posted, one entry each, so a Book can see which asset each charge belongs to rather than one lump nobody can unpick.
pub fn post_month<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let period = text(payload, "period");
    let Some(period) = periods::period_of(&period) else {
        return Err(refused(
            "Say which month to post depreciation for, as a period such as 2026-11.",
        ));
    };
    let Some(entry_date) = periods::last_day_of(&period) else {
        return Err(refused("That period does not read as a month."));
    };
    // a closed month is not written into: the charge belongs in the month that is still open
    if let Some(refusal) = periods::refusal_for(graph, scope, &entry_date)? {
        return Err(refused(&refusal));
    }
    let rows = rows_for(graph, scope, &period)?;
    if rows.is_empty() {
        return Err(refused(&format!(
            "The asset register holds no depreciation for {period}, so there is nothing to post. Record the month in the register first."
        )));
    }
    let accounts = accounts_for(graph, scope, payload)?;
    let names = asset_names(graph, scope)?;
    let created_at = text(payload, "created_at");
    let mut posted = Vec::new();
    let mut already = Vec::new();
    let mut total = Money::zero();
    for row in &rows {
        match post_row(graph, scope, row, &accounts, &entry_date, &created_at)? {
            Some(stamped) => {
                total = total.plus(&money(&stamped, "depreciation_amount")?);
                posted.push(with_asset_name(stamped, &names));
            }
            None => already.push(with_asset_name(
                json!({
                    "asset_id": field(row, "asset_id"),
                    "period": field(row, "period"),
                    "depreciation_amount": field(row, "depreciation_amount"),
                    "journal_entry_id": field(row, "journal_entry_id"),
                }),
                &names,
            )),
        }
    }
    Ok(json!({
        "period": period,
        "total": total.to_fixed(2),
        "posted": posted,
        "already_posted": already,
    }))
}

/// Post the charge for one register row, named by its id.
pub fn post_one<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let row_id = text(payload, "depreciation_entry_id");
    if row_id.trim().is_empty() {
        return Err(refused(
            "Say which month's depreciation to post, by the row the register recorded.",
        ));
    }
    let row = Records::new(graph, DEPRECIATION_ENTRY).get(scope, row_id.trim())?;
    let period = field(&row, "period").to_string();
    let entry_date = periods::last_day_of(&period)
        .ok_or_else(|| refused("That row's period does not read as a month."))?;
    if let Some(refusal) = periods::refusal_for(graph, scope, &entry_date)? {
        return Err(refused(&refusal));
    }
    let accounts = accounts_for(graph, scope, payload)?;
    match post_row(
        graph,
        scope,
        &row,
        &accounts,
        &entry_date,
        &text(payload, "created_at"),
    )? {
        Some(stamped) => Ok(stamped),
        None => Err(refused(&format!(
            "The depreciation for {period} has already been posted to the ledger."
        ))),
    }
}

/// The charges this Book has posted, newest month first, for one asset when asked.
pub fn list_posted<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let asset = text(query, "asset_id");
    let period = text(query, "period");
    let mut rows: Vec<Value> = Records::new(graph, DEPRECIATION_ENTRY)
        .list(scope, 5000)?
        .into_iter()
        .filter(|row| !field(row, "journal_entry_id").trim().is_empty())
        .filter(|row| asset.trim().is_empty() || field(row, "asset_id") == asset.trim())
        .filter(|row| period.trim().is_empty() || field(row, "period") == period.trim())
        .collect();
    rows.sort_by(|a, b| {
        field(b, "period")
            .cmp(field(a, "period"))
            .then_with(|| field(a, "asset_id").cmp(field(b, "asset_id")))
    });
    let names = asset_names(graph, scope)?;
    Ok(Value::Array(
        rows.into_iter()
            .map(|row| with_asset_name(row, &names))
            .collect(),
    ))
}

/// What the Book still owes the register: the months recorded and not yet in the books.
///
/// A Book whose register has run ahead of its ledger is not wrong, it is late, and this says by how much rather than
/// leaving somebody to compare two lists by eye.
pub fn unposted<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    let rows = Records::new(graph, DEPRECIATION_ENTRY).list(scope, 5000)?;
    let mut waiting: Vec<Value> = rows
        .into_iter()
        .filter(|row| field(row, "journal_entry_id").trim().is_empty())
        .collect();
    waiting.sort_by(|a, b| {
        field(a, "period")
            .cmp(field(b, "period"))
            .then_with(|| field(a, "asset_id").cmp(field(b, "asset_id")))
    });
    let mut total = Money::zero();
    for row in &waiting {
        total = total.plus(&money(row, "depreciation_amount")?);
    }
    let names = asset_names(graph, scope)?;
    Ok(json!({
        "unposted": waiting
            .into_iter()
            .map(|row| with_asset_name(row, &names))
            .collect::<Vec<Value>>(),
        "total": total.to_fixed(2),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vimbai_store::fake::MemoryGraph;
    use vimbai_store::write::remember;
    use vimbai_store::DEPRECIATION_ENTRY as REGISTER_ROWS;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn business() -> (MemoryGraph, BookScope) {
        let graph = MemoryGraph::new();
        let scope = scope();
        crate::chart::seed_opening_chart(&graph, &scope, "business").expect("a chart");
        (graph, scope)
    }

    /// A month of wear the asset register has already worked out.
    fn register_row(
        graph: &MemoryGraph,
        scope: &BookScope,
        asset_id: &str,
        period: &str,
        amount: &str,
    ) -> Value {
        remember(
            graph,
            scope,
            REGISTER_ROWS,
            json!({"asset_id": asset_id, "period": period,
                    "depreciation_amount": amount, "method": "straight_line"}),
        )
        .expect("a register row")
    }

    fn lines_of(graph: &MemoryGraph, scope: &BookScope, entry_id: &str) -> Vec<Value> {
        crate::ledger_book::get_entry(graph, scope, entry_id).expect("the entry")["lines"]
            .as_array()
            .cloned()
            .expect("lines")
    }

    fn line(lines: &[Value], account: &str) -> Value {
        lines
            .iter()
            .find(|one| field(one, "account_number") == account)
            .unwrap_or_else(|| panic!("no line on {account}"))
            .clone()
    }

    /// The wear is a cost and the thing stays in the books at what it cost: the charge is an expense and the same amount
    /// sits in accumulated depreciation beside the asset rather than being taken off it.
    #[test]
    fn a_month_of_wear_is_a_cost_and_the_asset_keeps_its_price() {
        let (graph, scope) = business();
        let row = register_row(&graph, &scope, "asset-1", "2026-11", "250.00");
        let posted = post_month(&graph, &scope, &json!({"period": "2026-11"})).expect("posted");

        assert_eq!(field(&posted, "total"), "250.00");
        let charged = posted["posted"].as_array().expect("a list");
        assert_eq!(charged.len(), 1);
        let entry_id = field(&charged[0], "journal_entry_id").to_string();
        let lines = lines_of(&graph, &scope, &entry_id);
        // the accounts are found by what the chart calls them, not by a number somebody has to remember
        assert_eq!(
            money(&line(&lines, "6200"), "debit").expect("the charge"),
            Money::parse("250.00").expect("250")
        );
        assert_eq!(
            money(&line(&lines, "1510"), "credit").expect("the wear"),
            Money::parse("250.00").expect("250")
        );
        assert_eq!(field(&row, "period"), "2026-11");
    }

    /// The charge is dated at the end of the month it belongs to, because a month has no single day.
    #[test]
    fn the_charge_is_dated_at_the_end_of_its_month() {
        let (graph, scope) = business();
        register_row(&graph, &scope, "asset-1", "2026-02", "100.00");
        let posted = post_month(&graph, &scope, &json!({"period": "2026-02"})).expect("posted");
        let entry_id = field(&posted["posted"][0], "journal_entry_id").to_string();
        let entry = crate::ledger_book::get_entry(&graph, &scope, &entry_id).expect("the entry");
        assert_eq!(field(&entry, "entry_date"), "2026-02-28");
    }

    /// A second run of the same month posts nothing: the row carries the entry that posted it, so the wear cannot be
    /// charged twice by somebody pressing the button again.
    #[test]
    fn the_same_month_cannot_be_posted_twice() {
        let (graph, scope) = business();
        register_row(&graph, &scope, "asset-1", "2026-11", "250.00");
        post_month(&graph, &scope, &json!({"period": "2026-11"})).expect("posted");
        let again = post_month(&graph, &scope, &json!({"period": "2026-11"})).expect("run again");

        assert_eq!(field(&again, "total"), "0.00");
        assert!(again["posted"].as_array().expect("a list").is_empty());
        assert_eq!(again["already_posted"].as_array().expect("a list").len(), 1);
    }

    /// Several assets in one month are one entry each, so a reader can see which asset each charge belongs to.
    #[test]
    fn every_asset_in_a_month_gets_its_own_entry() {
        let (graph, scope) = business();
        register_row(&graph, &scope, "asset-1", "2026-11", "250.00");
        register_row(&graph, &scope, "asset-2", "2026-11", "80.00");
        register_row(&graph, &scope, "asset-3", "2026-10", "10.00");
        let posted = post_month(&graph, &scope, &json!({"period": "2026-11"})).expect("posted");

        assert_eq!(field(&posted, "total"), "330.00");
        assert_eq!(posted["posted"].as_array().expect("a list").len(), 2);
        let untouched = Records::new(&graph, REGISTER_ROWS)
            .list(&scope, 100)
            .expect("rows")
            .into_iter()
            .find(|row| field(row, "asset_id") == "asset-3")
            .expect("the October row");
        assert!(
            field(&untouched, "journal_entry_id").is_empty(),
            "another month's row is not posted by this one"
        );
    }

    #[test]
    fn a_month_the_register_never_worked_out_is_refused_rather_than_posted_empty() {
        let (graph, scope) = business();
        let refused = post_month(&graph, &scope, &json!({"period": "2026-11"}))
            .expect_err("nothing recorded");
        assert!(format!("{refused}").contains("nothing to post"));
    }

    #[test]
    fn a_period_that_does_not_read_as_a_month_is_refused() {
        let (graph, scope) = business();
        let refused =
            post_month(&graph, &scope, &json!({"period": "November"})).expect_err("not a month");
        assert!(format!("{refused}").contains("as a period such as"));
    }

    /// A closed month is not written into: the charge belongs in the month that is still open.
    #[test]
    fn a_closed_month_is_refused() {
        let (graph, scope) = business();
        register_row(&graph, &scope, "asset-1", "2026-11", "250.00");
        crate::periods::close_period(
            &graph,
            &scope,
            &json!({"period": "2026-11", "closes_on": "2026-11-30"}),
        )
        .expect("closed");
        let refused =
            post_month(&graph, &scope, &json!({"period": "2026-11"})).expect_err("closed month");
        assert!(
            format!("{refused}").contains("closed") || format!("{refused}").contains("2026-11")
        );
    }

    /// A Book that has no account for the wear says so instead of posting the cost to nothing.
    #[test]
    fn a_book_without_the_accounts_is_refused_naming_what_is_missing() {
        let graph = MemoryGraph::new();
        let scope = scope();
        // only the accounts every set of books has, which include neither depreciation nor accumulated depreciation
        crate::chart::seed_opening_chart(&graph, &scope, "personal").expect("a chart");
        register_row(&graph, &scope, "asset-1", "2026-11", "250.00");
        let refused =
            post_month(&graph, &scope, &json!({"period": "2026-11"})).expect_err("no account");
        let words = format!("{refused}");
        assert!(words.contains("no account for depreciation"), "{words}");
    }

    /// One row can be posted on its own, and the register's own row keeps the entry that posted it.
    #[test]
    fn one_rows_charge_can_be_posted_by_its_id_and_is_then_marked() {
        let (graph, scope) = business();
        let row = register_row(&graph, &scope, "asset-9", "2026-11", "125.00");
        let row_id = field(&row, "id").to_string();
        let posted =
            post_one(&graph, &scope, &json!({"depreciation_entry_id": row_id})).expect("posted");
        assert!(!field(&posted, "journal_entry_id").is_empty());

        let again = post_one(&graph, &scope, &json!({"depreciation_entry_id": row_id}))
            .expect_err("already posted");
        assert!(format!("{again}").contains("already been posted"));
    }

    /// A charge is spoken about as the thing it belongs to, not by an id nobody recognises: the register owns the name and
    /// this only reads it.
    #[test]
    fn a_charge_is_named_after_the_thing_it_is_about() {
        let (graph, scope) = business();
        remember(
            &graph,
            &scope,
            FIXED_ASSET,
            json!({"asset_name": "Delivery van", "acquisition_cost": "12000.00"}),
        )
        .expect("an asset");
        let asset_id = Records::new(&graph, FIXED_ASSET)
            .list(&scope, 10)
            .expect("assets")[0]["id"]
            .as_str()
            .expect("an id")
            .to_string();
        register_row(&graph, &scope, &asset_id, "2026-11", "200.00");

        let waiting = unposted(&graph, &scope).expect("read");
        assert_eq!(field(&waiting["unposted"][0], "asset_name"), "Delivery van");

        let posted = post_month(&graph, &scope, &json!({"period": "2026-11"})).expect("posted");
        assert_eq!(field(&posted["posted"][0], "asset_name"), "Delivery van");
    }

    /// What the register has recorded and the books have not is stated as a figure, so a Book that is behind says by how
    /// much rather than leaving somebody to compare two lists by eye.
    #[test]
    fn what_is_recorded_and_not_yet_in_the_books_is_counted() {
        let (graph, scope) = business();
        register_row(&graph, &scope, "asset-1", "2026-10", "100.00");
        register_row(&graph, &scope, "asset-1", "2026-11", "100.00");
        post_month(&graph, &scope, &json!({"period": "2026-10"})).expect("posted");

        let waiting = unposted(&graph, &scope).expect("read");
        assert_eq!(field(&waiting, "total"), "100.00");
        assert_eq!(waiting["unposted"].as_array().expect("a list").len(), 1);

        let posted = list_posted(&graph, &scope, &json!({})).expect("listed");
        assert_eq!(posted.as_array().expect("a list").len(), 1);
    }
}
