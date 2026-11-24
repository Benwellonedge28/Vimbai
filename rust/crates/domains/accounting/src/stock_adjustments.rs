//! Stock that is not what the books say it is.
//!
//! A business counts its stock and finds a different figure from the one its books carry. That difference is real and has
//! to be said plainly, because stock is money: it was paid for, it sits in the accounts as something owned, and a count
//! that comes up short means part of what the books claim to own is gone.
//!
//! Two things a count can find, and both are the same journal with the sides swapped:
//!
//! * *down* - stock damaged, stolen, spoiled, or gone out of date. Dr Stock Adjustments, Cr Inventory: what the business
//!   owned comes down and the month carries the cost of it. It is not a purchase return, because no supplier owes anything;
//!   it is not a cost of sales, because nothing was sold;
//! * *up* - a count finds more than the books carry, usually because something was written down too hard or a delivery was
//!   never recorded. Dr Inventory, Cr Stock Adjustments: the business owns more than it thought and the cost it carried
//!   earlier comes back. It is credited to the same account the shortage was charged to, so the account says what counting
//!   has cost the business over time rather than hiding gains in a corner of sales.
//!
//! What this keeps, and why:
//!
//! * the count is the document: it names what was counted, which way it went, and what the difference comes to. The engine
//!   does not value stock by multiplying a quantity by a price, because it does not know the business's costing method and
//!   guessing one would put a figure in the books that nobody can defend. The person states the difference in money, which
//!   is what the count says;
//! * one adjustment per thing per day, so a second press cannot take the same shortage out of the books twice;
//! * a closed month is not written into: its profit has been reported;
//! * the accounts are found by what the chart calls them and by their type, never by number;
//! * nothing moves money. Counting stock never pays anybody or collects anything.

use serde_json::{json, Value};
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, STOCK_ADJUSTMENT};

use crate::periods;
use crate::subsidiary_journals::{account_named, must_be_open, positive, post};

/// Which way the count went: the books carry more than there is, or less.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Way {
    Down,
    Up,
}

impl Way {
    /// Reads the word the caller sent, in the words a person uses rather than the ledger's.
    fn of(payload: &Value) -> Result<Self, StoreError> {
        let said = text(payload, "direction").trim().to_lowercase();
        match said.as_str() {
            "down" | "short" | "shortage" | "less" => Ok(Way::Down),
            "up" | "surplus" | "more" => Ok(Way::Up),
            "" => Err(refused(
                "Say which way the count went: down if there is less stock than the books carry, up if there is more.",
            )),
            other => Err(refused(&format!(
                "{other} does not say which way the count went. Say down or up."
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Way::Down => "down",
            Way::Up => "up",
        }
    }
}

/// The day the count was taken, and the month it belongs to.
fn day_of(payload: &Value) -> Result<String, StoreError> {
    let stated = text(payload, "entry_date").trim().to_string();
    if stated.is_empty() {
        return Err(refused(
            "Say the day the stock was counted, as 2026-11-20. The month follows from it.",
        ));
    }
    if periods::month_of(&stated).is_none() {
        return Err(refused(&format!(
            "{stated} does not read as a day. Say it as 2026-11-20."
        )));
    }
    Ok(stated)
}

/// `POST /stock-adjustments`: record what a count found, taking stock down or putting it back up.
pub fn adjust(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let entry_date = day_of(payload)?;
    must_be_open(graph, scope, &entry_date)?;
    let item = text(payload, "item").trim().to_string();
    if item.is_empty() {
        return Err(refused(
            "Say what was counted: the item, or the reference the business knows it by.",
        ));
    }
    let way = Way::of(payload)?;
    let amount = positive(payload, "amount", "A stock difference")?;
    let stock = account_named(
        graph,
        scope,
        payload,
        "inventory_account",
        "inventory",
        "asset",
    )?;
    let cost = account_named(
        graph,
        scope,
        payload,
        "adjustment_account",
        "stock adjustment",
        "expense",
    )?;
    if stock == cost {
        return Err(refused(
            "The stock and the adjustment cannot be the same account: one is what the business owns and the other is what counting has cost it.",
        ));
    }
    let why = text(payload, "reason");
    let note = match (way, why.trim().is_empty()) {
        (Way::Down, true) => format!("Stock short: {item}"),
        (Way::Down, false) => format!("Stock short: {item} ({why})"),
        (Way::Up, true) => format!("Stock found: {item}"),
        (Way::Up, false) => format!("Stock found: {item} ({why})"),
    };
    // one adjustment per thing per day: the ledger refuses a reference it has already posted, so a second press cannot
    // take the same shortage out of the books twice
    let reference = format!("STK-{entry_date}-{item}");
    let dated = json!({"entry_date": entry_date, "created_at": text(payload, "created_at")});
    let description = match way {
        Way::Down => format!("Stock short: {item}"),
        Way::Up => format!("Stock found: {item}"),
    };
    // a shortage charges the month and brings stock down; a surplus puts stock back and takes the earlier charge back
    let (debit, credit) = match way {
        Way::Down => (&cost, &stock),
        Way::Up => (&stock, &cost),
    };
    let entry_id = post(
        graph,
        scope,
        &dated,
        description,
        &reference,
        "StockAdjustments",
        debit,
        credit,
        &amount,
        &note,
    )?;
    remember(
        graph,
        scope,
        STOCK_ADJUSTMENT,
        json!({
            "entry_date": entry_date,
            "item": item,
            "direction": way.as_str(),
            "amount": wire(&amount),
            "inventory_account": stock,
            "adjustment_account": cost,
            "reason": why,
            "journal_entry_id": entry_id,
            "notes": text(payload, "notes"),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /stock-adjustments`: what counting has found, newest first, narrowed by item or month when asked.
pub fn list_adjustments<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let item = text(query, "item");
    let period = text(query, "period");
    let mut rows: Vec<Value> = Records::new(graph, STOCK_ADJUSTMENT)
        .list(scope, 5000)?
        .into_iter()
        .filter(|row| {
            let item_said = item.trim().is_empty()
                || field(row, "item")
                    .to_lowercase()
                    .contains(&item.trim().to_lowercase());
            let period_said = period.trim().is_empty()
                || periods::month_of(field(row, "entry_date")).as_deref() == Some(period.trim());
            item_said && period_said
        })
        .collect();
    rows.sort_by(|a, b| {
        field(b, "entry_date")
            .cmp(field(a, "entry_date"))
            .then_with(|| field(a, "item").cmp(field(b, "item")))
    });
    Ok(Value::Array(rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vimbai_common::Money;
    use vimbai_store::fake::MemoryGraph;
    use vimbai_store::read::money;

    fn business() -> (MemoryGraph, BookScope) {
        let graph = MemoryGraph::new();
        let scope = BookScope::new("user-1", "book-1");
        crate::chart::seed_opening_chart(&graph, &scope, "business").expect("a chart");
        (graph, scope)
    }

    fn amount(text: &str) -> Money {
        Money::parse(text).expect("an amount")
    }

    fn entry_of(graph: &MemoryGraph, scope: &BookScope, written: &Value) -> Value {
        crate::ledger_book::get_entry(graph, scope, field(written, "journal_entry_id"))
            .expect("the entry")
    }

    fn line_on<'a>(lines: &'a [Value], account: &str) -> &'a Value {
        lines
            .iter()
            .find(|one| field(one, "account_number") == account)
            .unwrap_or_else(|| panic!("no line on {account}"))
    }

    fn short() -> Value {
        json!({"entry_date": "2026-11-20", "item": "Cement 50kg", "direction": "down",
               "amount": "180.00", "reason": "Water damage in the store"})
    }

    #[test]
    fn stock_that_is_short_comes_down_and_the_month_carries_it() {
        let (graph, scope) = business();
        let written = adjust(&graph, &scope, &short()).expect("an adjustment");

        assert_eq!(field(&written, "item"), "Cement 50kg");
        assert_eq!(field(&written, "direction"), "down");
        assert_eq!(field(&written, "amount"), "180.000000");

        let entry = entry_of(&graph, &scope, &written);
        // the cost is charged and what the business owns comes down: nothing was sold, so no cost of sales
        assert_eq!(
            money(
                line_on(entry["lines"].as_array().expect("lines"), "6400"),
                "debit"
            )
            .expect("a debit"),
            amount("180")
        );
        assert_eq!(
            money(
                line_on(entry["lines"].as_array().expect("lines"), "1200"),
                "credit"
            )
            .expect("a credit"),
            amount("180")
        );
        assert_eq!(field(&entry, "source_module"), "StockAdjustments");
        assert_eq!(field(&entry, "entry_date"), "2026-11-20");
    }

    #[test]
    fn stock_found_puts_it_back_and_takes_the_earlier_charge_back() {
        let (graph, scope) = business();
        let found = json!({"entry_date": "2026-11-21", "item": "Cement 50kg", "direction": "up",
                           "amount": "60.00", "reason": "Counted again, three bags were behind the pallet"});
        let written = adjust(&graph, &scope, &found).expect("an adjustment");

        let entry = entry_of(&graph, &scope, &written);
        let lines = entry["lines"].as_array().expect("lines");
        // the same pair the other way round: what the business owns goes back up and the charge comes back with it
        assert_eq!(
            money(line_on(lines, "1200"), "debit").expect("a debit"),
            amount("60")
        );
        assert_eq!(
            money(line_on(lines, "6400"), "credit").expect("a credit"),
            amount("60")
        );
    }

    #[test]
    fn the_same_count_cannot_be_recorded_twice_on_the_same_day() {
        let (graph, scope) = business();
        adjust(&graph, &scope, &short()).expect("an adjustment");

        let again = adjust(&graph, &scope, &short()).expect_err("a second adjustment");
        assert!(
            again.to_string().to_lowercase().contains("already"),
            "{again}"
        );
        assert_eq!(
            Records::new(&graph, STOCK_ADJUSTMENT)
                .list(&scope, 10)
                .expect("the records")
                .len(),
            1
        );
    }

    #[test]
    fn the_count_says_what_it_counted_which_way_it_went_and_what_it_comes_to() {
        let (graph, scope) = business();
        let nameless = json!({"entry_date": "2026-11-20", "direction": "down", "amount": "10.00"});
        let said = adjust(&graph, &scope, &nameless)
            .expect_err("no item")
            .to_string();
        assert!(said.contains("what was counted"), "{said}");

        let sideways = json!({"entry_date": "2026-11-20", "item": "Sugar", "direction": "sideways",
                              "amount": "10.00"});
        let said = adjust(&graph, &scope, &sideways)
            .expect_err("no way")
            .to_string();
        assert!(said.contains("does not say which way"), "{said}");

        let unsaid = json!({"entry_date": "2026-11-20", "item": "Sugar", "amount": "10.00"});
        let said = adjust(&graph, &scope, &unsaid)
            .expect_err("no direction")
            .to_string();
        assert!(said.contains("which way the count went"), "{said}");

        let nothing = json!({"entry_date": "2026-11-20", "item": "Sugar", "direction": "down",
                             "amount": "0"});
        let said = adjust(&graph, &scope, &nothing)
            .expect_err("nothing")
            .to_string();
        assert!(said.contains("more than nothing"), "{said}");
    }

    #[test]
    fn a_closed_month_is_not_written_into() {
        let (graph, scope) = business();
        crate::periods::close_period(
            &graph,
            &scope,
            &json!({"period": "2026-11", "closes_on": "2026-11-30"}),
        )
        .expect("closed");

        let said = adjust(&graph, &scope, &short())
            .expect_err("closed month")
            .to_string();
        assert!(said.to_lowercase().contains("closed"), "{said}");
        assert!(Records::new(&graph, STOCK_ADJUSTMENT)
            .list(&scope, 10)
            .expect("the records")
            .is_empty());
    }

    #[test]
    fn what_counting_found_is_listed_newest_first_and_narrows_by_item_and_month() {
        let (graph, scope) = business();
        adjust(&graph, &scope, &short()).expect("an adjustment");
        adjust(
            &graph,
            &scope,
            &json!({"entry_date": "2026-12-02", "item": "Sugar 2kg", "direction": "down", "amount": "25.00"}),
        )
        .expect("another adjustment");

        let all = list_adjustments(&graph, &scope, &json!({})).expect("a list");
        let rows = all.as_array().expect("rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(field(&rows[0], "item"), "Sugar 2kg");

        let cement = list_adjustments(&graph, &scope, &json!({"item": "cement"})).expect("a list");
        assert_eq!(cement.as_array().expect("rows").len(), 1);
        assert_eq!(field(&cement[0], "item"), "Cement 50kg");

        let november =
            list_adjustments(&graph, &scope, &json!({"period": "2026-11"})).expect("a list");
        assert_eq!(november.as_array().expect("rows").len(), 1);
        assert_eq!(field(&november[0], "item"), "Cement 50kg");
    }
}
