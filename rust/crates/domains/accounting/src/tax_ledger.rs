#![forbid(unsafe_code)]
//! The tax ledger: each tax the Book charges, what it took in, what was taken back, and what was paid over.
//!
//! The summary in [`crate::tax`] answers one question over a period: what was collected, what can be reclaimed, what is
//! owed. This is the account behind that figure. Each tax the Book defines posts to its own account, so each gets its own
//! running record: what it was owed at the start of the window, what the window charged, what was taken back, what was
//! paid to the authority, and what is owed now, with every movement kept so a total can be followed to the entry that
//! made it.
//!
//! Two things are deliberate:
//!
//! * the figures are the ledger's own. Nothing here multiplies a total by a percentage: a rate this file chose would be a
//!   claim about somebody's tax position that the books do not support. The Book's rate is reported beside the account so
//!   a person can see which tax the account belongs to, not so a figure can be derived from it.
//! * paying the tax over is a posting, not a note. It moves the liability out of the tax account and out of the bank, so
//!   what is owed afterwards is what the ledger says rather than what somebody remembers paying.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, money_or_nothing, refused, text};
use vimbai_store::{BookScope, Graph, Records, StoreError, JOURNAL_ENTRY, JOURNAL_LINE};

use crate::ledger_book;
use crate::subsidiary_journals::{account_name, account_named, must_be_open, positive, post};
use crate::tax::tax_accounts;
use crate::taxes;

/// The module that posts a payment of tax over, so its own entries can be told from a purchase's reclaim.
///
/// The distinction matters: a debit to a tax account is either tax the business is taking back or money it has handed
/// over, and only the source of the entry says which. Reading the side alone would report a payment as a reclaim and
/// leave the liability looking unpaid.
pub const MODULE: &str = "TaxLedger";

/// How many entries the ledger reads. A Book that has posted more tax than this in its life is one where the window
/// should be narrowed rather than the reading widened.
const REACH: usize = 4000;

/// What one movement in a tax account is, in the words a person reading the ledger would use.
fn kind_of(source: &str, debit: &Money) -> &'static str {
    let paid_by_this_ledger = source == MODULE;
    match (paid_by_this_ledger, debit.is_zero()) {
        // money handed to the tax authority, and that payment undone
        (true, false) => "paid",
        (true, true) => "returned",
        // tax the Book charged, and tax it took back: a purchase's reclaim or a sale reversed
        (false, false) => "taken_back",
        (false, true) => "charged",
    }
}

/// One tax account's movements, oldest first, with what each did to the balance owed.
fn movements_for(entries: &[Value], account: &str) -> Result<Vec<Value>, StoreError> {
    let mut rows = Vec::new();
    for entry in entries {
        for line in entry
            .get("lines")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            if field(line, "account_number") != account {
                continue;
            }
            let debit = money_or_nothing(line, "debit")?;
            let credit = money_or_nothing(line, "credit")?;
            if debit.is_zero() && credit.is_zero() {
                continue;
            }
            rows.push(json!({
                "entry_id": field(entry, "id"),
                "entry_date": field(entry, "entry_date"),
                "description": field(entry, "description"),
                "reference": field(entry, "reference_number"),
                "source_module": field(entry, "source_module"),
                "kind": kind_of(field(entry, "source_module"), &debit),
                "debit": wire(&debit),
                "credit": wire(&credit),
            }));
        }
    }
    Ok(rows)
}

/// The Book's taxes, by the account each one is owed in, so an account can be named by the tax it holds.
///
/// A tax names two accounts, one for what it collects and one for what it reclaims; a Book that charges the same tax on
/// both sides keeps one account for both, and that is the account this maps.
fn taxes_by_account<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
) -> Result<BTreeMap<String, (String, String)>, StoreError> {
    let defined = taxes::list_taxes(graph, scope, &json!({}))?;
    let mut by_account: BTreeMap<String, (String, String)> = BTreeMap::new();
    for tax in defined
        .get("taxes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let name = field(tax, "name").to_string();
        let rate = field(tax, "rate").to_string();
        for key in ["collected_account", "reclaim_account"] {
            let account = field(tax, key);
            if account.trim().is_empty() {
                continue;
            }
            by_account
                .entry(account.to_string())
                .or_insert_with(|| (name.clone(), rate.clone()));
        }
    }
    Ok(by_account)
}

/// `GET /accounting/tax-ledger/`: each tax's account, with its opening balance, what moved it and what it comes to.
///
/// The window is optional: with no dates the ledger runs from the Book's first entry to now, which is what somebody asking
/// "what have we got on VAT" wants. With dates it answers the same question for a period, and the opening balance is what
/// the account held before that period began.
pub fn tax_ledger<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let from = text(query, "from").trim().to_string();
    let until = text(query, "to").trim().to_string();
    if !from.is_empty() && !until.is_empty() && from > until {
        return Err(refused(&format!(
            "The period runs backwards: from {from} is after to {until}."
        )));
    }

    let held = tax_accounts(graph, scope)?;
    let named = taxes_by_account(graph, scope)?;
    // every account the Book defines a tax for is reported even before anything has posted to it, because a tax that
    // exists and has moved nothing is a fact about the Book; an account in the chart that no tax names is still a tax
    // account, so it is reported by what the chart calls it
    let mut numbers: Vec<String> = held.iter().map(|(number, _)| number.clone()).collect();
    for account in named.keys() {
        if !numbers.contains(account) {
            numbers.push(account.clone());
        }
    }
    numbers.sort();

    let mut entries: Vec<Value> = Records::new(graph, JOURNAL_ENTRY)
        .list_with_children(scope, REACH, JOURNAL_LINE)?
        .into_iter()
        .filter(|entry| field(entry, "status") == "posted")
        .collect();
    // oldest first, so the running balance reads the way the account was lived
    entries.sort_by(|one, two| {
        field(one, "entry_date")
            .cmp(field(two, "entry_date"))
            .then(field(one, "created_at").cmp(field(two, "created_at")))
    });

    let mut rows: Vec<Value> = Vec::new();
    let mut total = Buckets::default();
    for number in &numbers {
        let chart_name = held
            .iter()
            .find(|(held_number, _)| held_number == number)
            .map(|(_, name)| name.clone())
            .or_else(|| named.get(number).map(|(tax, _)| format!("{tax} payable")))
            .unwrap_or_default();
        let movements = movements_for(&entries, number)?;
        let mut buckets = Buckets::default();
        let mut running = Money::zero();
        let mut kept: Vec<Value> = Vec::new();
        for movement in movements {
            let date = field(&movement, "entry_date").to_string();
            let debit = money_or_nothing(&movement, "debit")?;
            let credit = money_or_nothing(&movement, "credit")?;
            // what a movement did to what is owed: a charge adds to it, anything else takes from it
            let effect = credit.minus(&debit);
            // the opening is everything before the window: a movement inside it moves the running balance and is
            // counted in what the window did, and one before it only sets where the window starts
            let inside = (from.is_empty() || date.as_str() >= from.as_str())
                && (until.is_empty() || date.as_str() <= until.as_str());
            if inside {
                match field(&movement, "kind") {
                    "charged" => buckets.charged = buckets.charged.plus(&credit),
                    "taken_back" => buckets.taken_back = buckets.taken_back.plus(&debit),
                    "paid" => buckets.paid = buckets.paid.plus(&debit),
                    _ => buckets.returned = buckets.returned.plus(&credit),
                }
                running = running.plus(&effect);
                let mut row = movement;
                row["balance"] = wire(&running);
                kept.push(row);
            } else if date.as_str() < from.as_str() {
                buckets.opening = buckets.opening.plus(&effect);
            }
        }
        let closing = buckets.closing();
        total.opening = total.opening.plus(&buckets.opening);
        total.charged = total.charged.plus(&buckets.charged);
        total.taken_back = total.taken_back.plus(&buckets.taken_back);
        total.paid = total.paid.plus(&buckets.paid);
        total.returned = total.returned.plus(&buckets.returned);
        let (tax_name, rate) = named
            .get(number)
            .cloned()
            .unwrap_or_else(|| (String::new(), String::new()));
        rows.push(json!({
            "account_number": number,
            "account_name": chart_name,
            // which tax this account holds, and the rate the Book states for it, so a reader can tell the accounts apart
            "tax_name": tax_name,
            "rate": rate,
            "opening": wire(&buckets.opening),
            "charged": wire(&buckets.charged),
            "taken_back": wire(&buckets.taken_back),
            "paid": wire(&buckets.paid),
            "returned": wire(&buckets.returned),
            "closing": wire(&closing),
            // a closing figure below nothing is money the authority owes the business, and it is said in words rather
            // than left for a reader to work out from a minus sign
            "is_reclaimable": closing.is_negative(),
            "movements": kept,
        }));
    }

    Ok(json!({
        "from": from,
        "to": until,
        "taxes": rows,
        "account_count": numbers.len(),
        "totals": {
            "opening": wire(&total.opening),
            "charged": wire(&total.charged),
            "taken_back": wire(&total.taken_back),
            "paid": wire(&total.paid),
            "returned": wire(&total.returned),
            "closing": wire(&total.closing()),
        },
        // said plainly, because a Book whose chart holds no tax account cannot have posted any tax at all
        "has_tax_account": !held.is_empty(),
        "note": if held.is_empty() {
            "This Book's chart holds no tax account, so nothing has been posted as tax. Seed the usual taxes, or add an \
             account for the tax this Book pays."
        } else {
            "Each tax is its own account, so what is owed is never one merged figure. What was paid over is the ledger's \
             own entry, not a note against the balance."
        },
    }))
}

/// What a window did to one tax account, kept together so the arithmetic is stated once.
struct Buckets {
    opening: Money,
    charged: Money,
    taken_back: Money,
    paid: Money,
    returned: Money,
}

impl Default for Buckets {
    fn default() -> Self {
        Buckets {
            opening: Money::zero(),
            charged: Money::zero(),
            taken_back: Money::zero(),
            paid: Money::zero(),
            returned: Money::zero(),
        }
    }
}

impl Buckets {
    fn closing(&self) -> Money {
        self.opening
            .plus(&self.charged)
            .plus(&self.returned)
            .minus(&self.taken_back)
            .minus(&self.paid)
    }
}

/// What is owed on one tax account right now: everything it has charged, less everything taken back or paid over.
fn owed_on<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    account: &str,
) -> Result<Money, StoreError> {
    let entries: Vec<Value> = Records::new(graph, JOURNAL_ENTRY)
        .list_with_children(scope, REACH, JOURNAL_LINE)?
        .into_iter()
        .filter(|entry| field(entry, "status") == "posted")
        .collect();
    let mut owed = Money::zero();
    for movement in movements_for(&entries, account)? {
        let debit = money_or_nothing(&movement, "debit")?;
        let credit = money_or_nothing(&movement, "credit")?;
        owed = owed.plus(&credit).minus(&debit);
    }
    Ok(owed)
}

/// `POST /accounting/tax-ledger/settle`: pay a tax over, so the liability leaves the books with the money.
///
/// This is the posting half of the tax ledger, and it is the half the Book did not have: the tax on a sale was recorded,
/// and what was handed to the authority afterwards lived only in somebody's memory, leaving the liability standing for
/// ever.
///
/// What this keeps, and why:
///
/// * the payment is stated by a person: the ledger says what is owed, and what was actually paid is a fact only the
///   person paying knows, so nothing here works out an instalment;
/// * a payment larger than what the account owes is refused, naming the figure: the difference would leave the tax
///   authority owing the business, which is a reclaim carried by the purchase that bears the tax, not a payment;
/// * the entry's reference names the day, the account and the amount, so pressing the same payment twice is refused by
///   the ledger rather than paying the same tax twice, while a second, different payment is its own entry;
/// * a closed month is not written into: its return has been made;
/// * nothing here moves money. Vimbai records a payment that happened; the bank did the paying.
pub fn settle_tax<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let entry_date = text(payload, "entry_date").trim().to_string();
    if entry_date.is_empty() {
        return Err(refused(
            "Say the day the tax was paid over, as 2026-11-20, in entry_date.",
        ));
    }
    must_be_open(graph, scope, &entry_date)?;
    let amount = positive(payload, "amount", "A tax payment")?;

    let held = tax_accounts(graph, scope)?;
    let asked = text(payload, "tax_account_number").trim().to_string();
    let tax_number = if !asked.is_empty() {
        if !held.iter().any(|(number, _)| number == &asked) {
            return Err(refused(&format!(
                "Account {asked} is not one of this Book's tax accounts, so nothing can be paid out of it."
            )));
        }
        asked
    } else {
        match held.len() {
            0 => {
                return Err(refused(
                    "This Book's chart holds no tax account, so there is no tax to pay over.",
                ))
            }
            1 => held[0].0.clone(),
            // one account per tax is the point of the design, so a Book with several says which is being paid rather
            // than settling a tax nobody chose
            _ => {
                return Err(refused(
                    "This Book keeps more than one tax account: say which one is being paid, in tax_account_number.",
                ))
            }
        }
    };
    let tax_name = held
        .iter()
        .find(|(number, _)| number == &tax_number)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| tax_number.clone());

    // the money it was paid from: named, or the Book's own bank, or the cash it keeps if it banks nowhere
    let money_number = match account_named(
        graph,
        scope,
        payload,
        "money_account_number",
        "bank",
        "asset",
    ) {
        Ok(number) => number,
        Err(refusal) => match text(payload, "money_account_number") {
            // a Book that keeps only cash pays its tax out of the cash it has
            named if named.trim().is_empty() => account_named(
                graph,
                scope,
                payload,
                "money_account_number",
                "cash",
                "asset",
            )?,
            _ => return Err(refusal),
        },
    };
    if money_number == tax_number {
        return Err(refused(
            "Paying a tax account out of itself moves nothing: say the bank or cash account the money left.",
        ));
    }
    let money_name = ledger_book::accounts(graph, scope)?
        .iter()
        .find(|account| field(account, "account_number") == money_number)
        .map(account_name)
        .unwrap_or_else(|| money_number.clone());

    let owed = owed_on(graph, scope, &tax_number)?;
    if owed.is_zero() || owed.is_negative() {
        return Err(refused(&format!(
            "{tax_name} has nothing owed on it, so there is nothing to pay over. What the business can reclaim is the \
             debit side of the same account."
        )));
    }
    if amount > owed {
        return Err(refused(&format!(
            "A payment of {} is more than the {} owed on {tax_name}. The difference would leave the tax authority owing \
             the business, which is a reclaim carried by the purchase that bears the tax, not a payment.",
            amount.to_fixed(2),
            owed.to_fixed(2)
        )));
    }

    let period = text(payload, "period").trim().to_string();
    let description = if period.is_empty() {
        format!("{tax_name} paid over")
    } else {
        format!("{tax_name} paid over for {period}")
    };
    let reference = match text(payload, "reference").trim() {
        // wire() is the JSON string the ledger stores, so the reference takes its text, not its quotes
        // the digits the ledger stores, so the reference a person reads is the one the books keep
        "" => format!("TAX-{entry_date}-{tax_number}-{}", amount.to_fixed(6)),
        given => given.to_string(),
    };
    let note = format!("Paid out of {money_name}");
    let entry_id = post(
        graph,
        scope,
        payload,
        description.clone(),
        &reference,
        MODULE,
        &tax_number,
        &money_number,
        &amount,
        &note,
    )?;

    let after = owed.minus(&amount);
    Ok(json!({
        "entry_date": entry_date,
        "amount": wire(&amount),
        "tax_account_number": tax_number,
        "tax_name": tax_name,
        "money_account_number": money_number,
        "money_account_name": money_name,
        "description": description,
        "reference": reference,
        "journal_entry_id": entry_id,
        // what the account owes now, so the answer says what the books say rather than only what was posted
        "owed_after": wire(&after),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::seed_opening_chart;
    use crate::periods::close_period;
    use crate::subsidiary_journals::{create_purchase, create_sale};
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    /// A business Book with its opening chart, one taxed sale and one taxed purchase.
    fn traded() -> MemoryGraph {
        let graph = MemoryGraph::new();
        seed_opening_chart(&graph, &scope(), "business").expect("a chart");
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
            &json!({"purchase_order_number": "PO-1", "entry_date": "2026-10-09",
                    "total_amount": "460.00", "tax_amount": "60.00"}),
        )
        .expect("a taxed purchase");
        graph
    }

    fn ledger(graph: &MemoryGraph, from: &str, to: &str) -> Value {
        tax_ledger(graph, &scope(), &json!({"from": from, "to": to})).expect("the ledger")
    }

    fn vat(answer: &Value) -> &Value {
        answer["taxes"]
            .as_array()
            .expect("taxes")
            .iter()
            .find(|row| row["account_number"] == json!("2100"))
            .expect("the VAT account")
    }

    #[test]
    fn a_tax_account_states_what_it_charged_took_back_and_now_owes() {
        let answer = ledger(&traded(), "2026-10-01", "2026-10-31");
        let vat = vat(&answer);
        assert_eq!(vat["account_name"], json!("VAT Payable"));
        assert_eq!(vat["charged"], json!("150.000000"));
        assert_eq!(vat["taken_back"], json!("60.000000"));
        assert_eq!(vat["closing"], json!("90.000000"));
        assert_eq!(answer["totals"]["closing"], json!("90.000000"));
        assert_eq!(answer["has_tax_account"], json!(true));
    }

    #[test]
    fn every_movement_is_kept_with_what_it_did_to_the_balance() {
        let answer = ledger(&traded(), "2026-10-01", "2026-10-31");
        let movements = vat(&answer)["movements"].as_array().expect("movements");
        assert_eq!(movements.len(), 2);
        // oldest first: the sale charged the tax, then the purchase took some back
        assert_eq!(movements[0]["kind"], json!("charged"));
        assert_eq!(movements[0]["reference"], json!("INV-1"));
        assert_eq!(movements[0]["balance"], json!("150.000000"));
        assert_eq!(movements[1]["kind"], json!("taken_back"));
        assert_eq!(movements[1]["balance"], json!("90.000000"));
    }

    #[test]
    fn a_window_opens_on_what_came_before_it() {
        let answer = ledger(&traded(), "2026-10-06", "2026-10-31");
        let vat = vat(&answer);
        // the sale on the 5th is before the window, so it opens the ledger instead of being counted in it
        assert_eq!(vat["opening"], json!("150.000000"));
        assert_eq!(vat["charged"], json!("0.000000"));
        assert_eq!(vat["closing"], json!("90.000000"));
        assert_eq!(vat["movements"].as_array().expect("movements").len(), 1);
    }

    #[test]
    fn paying_the_tax_over_leaves_the_books_with_the_money() {
        let graph = traded();
        let paid = settle_tax(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-05", "amount": "90.00", "period": "2026-10"}),
        )
        .expect("the payment");
        assert_eq!(paid["tax_account_number"], json!("2100"));
        assert_eq!(paid["money_account_number"], json!("1010"));
        assert_eq!(
            paid["description"],
            json!("VAT Payable paid over for 2026-10")
        );
        assert_eq!(paid["owed_after"], json!("0.000000"));

        let answer = ledger(&graph, "2026-10-01", "2026-11-30");
        let vat = vat(&answer);
        assert_eq!(vat["paid"], json!("90.000000"));
        assert_eq!(vat["closing"], json!("0.000000"));
        // the payment is a debit, and it is told apart from a reclaim by the entry that made it
        let movements = vat["movements"].as_array().expect("movements");
        assert_eq!(movements.last().expect("a payment")["kind"], json!("paid"));
    }

    #[test]
    fn paying_the_same_tax_twice_is_refused_by_the_ledger() {
        let graph = traded();
        let payload = json!({"entry_date": "2026-11-05", "amount": "90.00"});
        settle_tax(&graph, &scope(), &payload).expect("the first payment");
        let again = settle_tax(&graph, &scope(), &payload).expect_err("a repeat");
        assert!(again.to_string().contains("nothing owed on it"), "{again}");
    }

    #[test]
    fn a_payment_larger_than_what_is_owed_is_refused_naming_the_figure() {
        let graph = traded();
        let too_much = settle_tax(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-05", "amount": "100.00"}),
        )
        .expect_err("too much");
        let said = too_much.to_string();
        assert!(said.contains("more than the 90.00 owed"), "{said}");
        assert!(said.contains("reclaim"), "{said}");
    }

    #[test]
    fn a_tax_with_nothing_owed_has_nothing_to_pay_over() {
        let graph = MemoryGraph::new();
        seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        let nothing = settle_tax(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-05", "amount": "10.00"}),
        )
        .expect_err("nothing owed");
        assert!(
            nothing.to_string().contains("nothing owed on it"),
            "{nothing}"
        );
    }

    #[test]
    fn a_closed_month_is_not_written_into() {
        let graph = traded();
        close_period(
            &graph,
            &scope(),
            &json!({"period": "2026-10", "closes_on": "2026-11-05", "closed_by": "sam"}),
        )
        .expect("the close");
        let refused_month = settle_tax(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-10-20", "amount": "90.00"}),
        )
        .expect_err("a closed month");
        assert!(
            refused_month.to_string().contains("2026-10"),
            "{refused_month}"
        );
    }

    #[test]
    fn a_book_with_no_tax_account_says_so_rather_than_showing_nothing_owed() {
        let graph = MemoryGraph::new();
        let answer = ledger(&graph, "2026-10-01", "2026-10-31");
        assert_eq!(answer["has_tax_account"], json!(false));
        assert_eq!(answer["taxes"], json!([]));
        assert!(answer["note"]
            .as_str()
            .expect("a note")
            .contains("no tax account"));
        let cannot_pay = settle_tax(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-05", "amount": "10.00"}),
        )
        .expect_err("no tax account");
        assert!(
            cannot_pay.to_string().contains("no tax account"),
            "{cannot_pay}"
        );
    }

    #[test]
    fn a_period_that_runs_backwards_is_refused() {
        let backwards = tax_ledger(
            &traded(),
            &scope(),
            &json!({"from": "2026-10-31", "to": "2026-10-01"}),
        )
        .expect_err("backwards");
        assert!(
            backwards.to_string().contains("runs backwards"),
            "{backwards}"
        );
    }

    #[test]
    fn a_payment_must_say_the_day_and_an_amount() {
        let graph = traded();
        let no_day = settle_tax(&graph, &scope(), &json!({"amount": "90.00"})).expect_err("no day");
        assert!(no_day.to_string().contains("entry_date"), "{no_day}");
        let no_amount = settle_tax(&graph, &scope(), &json!({"entry_date": "2026-11-05"}))
            .expect_err("no amount");
        assert!(no_amount.to_string().contains("amount"), "{no_amount}");
        let nothing = settle_tax(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-05", "amount": "0"}),
        )
        .expect_err("nothing");
        assert!(
            nothing.to_string().contains("more than nothing"),
            "{nothing}"
        );
    }

    #[test]
    fn a_payment_cannot_leave_the_tax_account_for_itself() {
        let graph = traded();
        let itself = settle_tax(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-05", "amount": "10.00",
                    "money_account_number": "2100"}),
        )
        .expect_err("itself");
        assert!(itself.to_string().contains("moves nothing"), "{itself}");
    }

    #[test]
    fn a_book_that_banks_nowhere_pays_its_tax_out_of_the_cash_it_keeps() {
        let graph = MemoryGraph::new();
        seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        // a Book whose only money account is cash, which is where the money actually left
        crate::ledger_book::update_account(
            &graph,
            &scope(),
            "1010",
            &json!({"name": "Money Held"}),
        )
        .expect("renaming the bank account");
        create_sale(
            &graph,
            &scope(),
            &json!({"invoice_number": "INV-2", "entry_date": "2026-10-05",
                    "total_amount": "1150.00", "tax_amount": "150.00"}),
        )
        .expect("a taxed sale");
        let paid = settle_tax(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-05", "amount": "150.00"}),
        )
        .expect("paid from cash");
        assert_eq!(paid["money_account_number"], json!("1000"));
        assert_eq!(paid["money_account_name"], json!("Cash"));
    }

    #[test]
    fn a_book_with_several_tax_accounts_says_which_one_is_being_paid() {
        let graph = traded();
        crate::ledger_book::create_account(
            &graph,
            &scope(),
            &json!({"account_number": "2110", "name": "PAYE Tax Payable",
                    "account_type": "liability", "normal_balance": "credit"}),
        )
        .expect("a second tax account");
        let unnamed = settle_tax(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-05", "amount": "10.00"}),
        )
        .expect_err("which one");
        assert!(
            unnamed.to_string().contains("more than one tax account"),
            "{unnamed}"
        );
        // named, it posts against the account the person chose
        let named = settle_tax(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-05", "amount": "10.00",
                    "tax_account_number": "2100"}),
        )
        .expect("the named account");
        assert_eq!(named["tax_account_number"], json!("2100"));
    }

    #[test]
    fn a_tax_the_book_defines_is_reported_before_anything_posts_to_it() {
        let graph = MemoryGraph::new();
        seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        crate::taxes::save_tax(
            &graph,
            &scope(),
            &json!({"name": "Withholding Tax", "rate": "10", "scope": "purchases",
                    "reclaim_account": "2100", "effective_from": "2026-01-01"}),
        )
        .expect("a tax");
        let answer = ledger(&graph, "2026-10-01", "2026-10-31");
        let vat = vat(&answer);
        // the account is named by the tax that holds it, and it reads as nothing moved rather than as no tax at all
        assert_eq!(vat["tax_name"], json!("Withholding Tax"));
        assert_eq!(vat["rate"], json!("10.0000"));
        assert_eq!(vat["closing"], json!("0.000000"));
    }
}
