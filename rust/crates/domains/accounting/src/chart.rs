#![forbid(unsafe_code)]
//! The chart of accounts a Book opens with, so a new Book can be written in before it is configured.
//!
//! An empty Book is not a small inconvenience. A journal entry may only name accounts the Book owns, so a person who
//! created a Book and posted their first sale was told "account number 4000 not found" and had to invent a chart before
//! they could record anything. Every accounting product answers that by opening the Book with a standard chart and
//! letting the bookkeeper rename or extend it later.
//!
//! Three rules hold this together:
//!
//! * **One chart per kind of Book.** The five tiers are five different kinds of bookkeeping, not five names for the same
//!   one. A household does not have a cost of sales; a group savings scheme collects member contributions it owes back; a
//!   nonprofit books donations rather than sales.
//! * **Seeding never overwrites.** It runs once when a Book is opened and it adds only numbers the Book does not already
//!   hold, so a chart someone has already edited is left alone.
//! * **Ordinary accounts only.** These are the accounts a bookkeeper would name themselves on day one. Nothing here
//!   depends on a jurisdiction, a tax regime or a reporting standard; those arrive as extra accounts, not as defaults.

use serde_json::{json, Value};
use vimbai_store::read::text;
use vimbai_store::{BookScope, Graph, Records, StoreError, LEDGER_ACCOUNT};

/// One account in an opening chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StandardAccount {
    /// The number the bookkeeper will quote on an entry.
    pub number: &'static str,
    pub name: &'static str,
    /// `asset`, `liability`, `equity`, `revenue` or `expense`.
    pub account_type: &'static str,
    /// The side a positive balance sits on.
    pub normal_balance: &'static str,
}

const fn account(
    number: &'static str,
    name: &'static str,
    account_type: &'static str,
    normal_balance: &'static str,
) -> StandardAccount {
    StandardAccount {
        number,
        name,
        account_type,
        normal_balance,
    }
}

/// The accounts every Book opens with, whatever it is for.
///
/// These are the five elements of any set of accounts: something you own, something you owe, what is left for the owner,
/// what came in and what went out.
pub const CORE: [StandardAccount; 8] = [
    account("1000", "Cash", "asset", "debit"),
    account("1010", "Bank", "asset", "debit"),
    account("2000", "Accounts Payable", "liability", "credit"),
    account("3000", "Owner's Equity", "equity", "credit"),
    account("4000", "Sales", "revenue", "credit"),
    account("5000", "Cost of Sales", "expense", "debit"),
    account("6100", "Rent", "expense", "debit"),
    account("6300", "Office Expenses", "expense", "debit"),
];

/// What a business books that the other tiers do not.
///
/// The two return accounts are here because goods coming back is ordinary trading: a sale taken back needs somewhere to
/// go that is not Sales, and goods returned to a supplier needs somewhere that is not Cost of Sales.
pub const BUSINESS: [StandardAccount; 18] = [
    account("1100", "Accounts Receivable", "asset", "debit"),
    account("1200", "Inventory", "asset", "debit"),
    // money already paid for something not yet used: it is the business's, but it is not this month's cost
    account("1300", "Prepaid Expenses", "asset", "debit"),
    account("1500", "Equipment", "asset", "debit"),
    // what an asset has been used up by is kept beside it rather than taken off it, so a reader can see both what the
    // business paid for the thing and how much of it is gone
    account("1510", "Accumulated Depreciation", "asset", "credit"),
    account("2100", "VAT Payable", "liability", "credit"),
    // a cost used this month that nobody has billed yet: the month carries it and the business owes it
    account("2200", "Accrued Expenses", "liability", "credit"),
    account("4050", "Sales Returns", "revenue", "debit"),
    // A business that trades in another money makes and loses money on the rate itself. That is not trading income, so
    // it gets its own pair rather than being folded into sales or costs: a gain is revenue, a loss is an expense, and a
    // reader can see what the currency did apart from what the business did.
    account("4800", "Exchange Gain", "revenue", "credit"),
    account("5050", "Purchases Returns", "expense", "credit"),
    account("5800", "Exchange Loss", "expense", "debit"),
    // the wear on what the business keeps is a cost of trading, and it belongs to a business rather than a household
    account("6200", "Depreciation", "expense", "debit"),
    // what counting the stock has cost the business: a shortage is charged here and a surplus found is credited back to it,
    // so the account says what counting has done over time rather than hiding a gain among the sales
    account("6400", "Stock Adjustments", "expense", "debit"),
    // a customer who will not pay is a cost of trading, not a loss of sales: the money was earned and is now gone, and a
    // reader of the accounts is entitled to see it apart from the trading that earned it
    account("6350", "Bad Debts", "expense", "debit"),
    // what it costs to employ people, and what is owed to them and on their behalf once a month's wages are worked out.
    // Three accounts rather than one because the three are different facts: what the work cost, what the staff are still
    // owed, and what the business holds for somebody else - the revenue authority, the pension fund, a garnishee.
    account("6250", "Salaries and Wages", "expense", "debit"),
    account("6260", "Employer Contributions", "expense", "debit"),
    account("2250", "Net Pay Payable", "liability", "credit"),
    account(
        "2260",
        "Statutory Deductions Payable",
        "liability",
        "credit",
    ),
];

/// A person's own money, and what living costs.
pub const PERSONAL: [StandardAccount; 4] = [
    account("4100", "Salary and Wages", "revenue", "credit"),
    account("6400", "Living Expenses", "expense", "debit"),
    account("6500", "Transport", "expense", "debit"),
    account("6600", "Medical", "expense", "debit"),
];

/// A household: the bills one home carries, and the savings it holds.
pub const HOUSEHOLD: [StandardAccount; 4] = [
    account("6700", "Household Bills", "expense", "debit"),
    account("6800", "School Fees", "expense", "debit"),
    account("1700", "Household Savings", "asset", "debit"),
    account("2200", "Loans Payable", "liability", "credit"),
];

/// A savings group: it holds members' money and owes it back.
pub const GROUP: [StandardAccount; 4] = [
    account("2300", "Member Contributions Held", "liability", "credit"),
    account("1800", "Group Fund", "asset", "debit"),
    account("4200", "Interest Earned", "revenue", "credit"),
    account("5900", "Fines and Charges", "revenue", "credit"),
];

/// A nonprofit: money given rather than earned, and what it is spent on.
pub const NONPROFIT: [StandardAccount; 4] = [
    account("4300", "Donations Received", "revenue", "credit"),
    account("4400", "Grants Received", "revenue", "credit"),
    account("7000", "Programme Costs", "expense", "debit"),
    account("7100", "Fundraising Costs", "expense", "debit"),
];

/// The chart a Book of this tier opens with, in number order and without a repeated number.
///
/// A tier the engine does not know gets the core chart: opening a Book with the accounts every set of books has is
/// better than opening one with none because the label was unfamiliar.
pub fn opening_chart(tier: &str) -> Vec<StandardAccount> {
    let mut chart: Vec<StandardAccount> = CORE.to_vec();
    let extra: &[StandardAccount] = match tier {
        "business" => &BUSINESS,
        "personal" => &PERSONAL,
        "household" => &HOUSEHOLD,
        "group" => &GROUP,
        "nonprofit" => &NONPROFIT,
        _ => &[],
    };
    chart.extend_from_slice(extra);
    chart.sort_by(|one, two| one.number.cmp(two.number));
    chart.dedup_by(|one, two| one.number == two.number);
    chart
}

/// The numbers a Book of this tier opens with, for a caller that only needs to say what was added.
pub fn opening_numbers(tier: &str) -> Vec<&'static str> {
    opening_chart(tier)
        .into_iter()
        .map(|one| one.number)
        .collect()
}

/// Give a Book the accounts it opens with, and answer how many were added.
///
/// This is safe to run twice: it adds only the numbers the Book does not already hold, so a chart somebody has edited
/// keeps their edits and gains only what is missing. It never renames an account, because a rename is a bookkeeper's
/// decision about their own books.
pub fn seed_opening_chart<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    tier: &str,
) -> Result<usize, StoreError> {
    let existing: Vec<String> = Records::new(graph, LEDGER_ACCOUNT)
        .list(scope, 2000)?
        .iter()
        .map(|row| text(row, "account_number"))
        .collect();

    let mut added = 0;
    for standard in opening_chart(tier) {
        if existing.iter().any(|held| held == standard.number) {
            continue;
        }
        let body = json!({
            "account_number": standard.number,
            "name": standard.name,
            "account_type": standard.account_type,
            "normal_balance": standard.normal_balance,
            "description": "opening chart",
            "parent_account_number": Value::Null,
            "created_at": "",
            "updated_at": "",
        });
        Records::new(graph, LEDGER_ACCOUNT).create(scope, &body)?;
        added += 1;
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn every_tier_opens_with_the_accounts_every_set_of_books_has() {
        for tier in ["personal", "household", "group", "business", "nonprofit"] {
            let chart = opening_chart(tier);
            for core in CORE {
                assert!(
                    chart.contains(&core),
                    "{tier} is missing {} {}",
                    core.number,
                    core.name
                );
            }
        }
    }

    #[test]
    fn a_tier_gets_its_own_accounts_and_no_others() {
        let business = opening_numbers("business");
        assert!(business.contains(&"1100"), "{business:?}");
        assert!(!business.contains(&"4300"), "{business:?}");

        let nonprofit = opening_numbers("nonprofit");
        assert!(nonprofit.contains(&"4300"), "{nonprofit:?}");
        assert!(!nonprofit.contains(&"1100"), "{nonprofit:?}");

        let group = opening_numbers("group");
        assert!(group.contains(&"2300"), "{group:?}");
        assert!(!group.contains(&"7000"), "{group:?}");
    }

    #[test]
    fn a_chart_is_in_number_order_and_never_repeats_a_number() {
        for tier in [
            "personal",
            "household",
            "group",
            "business",
            "nonprofit",
            "strange",
        ] {
            let numbers = opening_numbers(tier);
            let mut sorted = numbers.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(
                numbers, sorted,
                "{tier} is out of order or repeats a number"
            );
        }
    }

    #[test]
    fn an_unknown_tier_still_opens_with_the_core_chart() {
        assert_eq!(opening_numbers("charity"), opening_numbers("strange"));
        assert!(opening_numbers("strange").contains(&"4000"));
    }

    #[test]
    fn seeding_a_book_writes_the_chart_and_running_it_again_adds_nothing() {
        let graph = MemoryGraph::new();
        let added = seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        assert_eq!(added, opening_numbers("business").len());

        // the second run finds everything already there
        let again = seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        assert_eq!(again, 0, "seeding twice must not duplicate the chart");

        let held = Records::new(&graph, LEDGER_ACCOUNT)
            .list(&scope(), 100)
            .expect("accounts");
        assert_eq!(held.len(), opening_numbers("business").len());
        assert!(held
            .iter()
            .any(|row| text(row, "account_number") == "1000" && text(row, "name") == "Cash"));
    }

    #[test]
    fn a_chart_somebody_has_edited_keeps_their_work() {
        let graph = MemoryGraph::new();
        Records::new(&graph, LEDGER_ACCOUNT)
            .create(
                &scope(),
                &json!({"account_number": "1000", "name": "Petty Cash",
                        "account_type": "asset", "normal_balance": "debit"}),
            )
            .expect("their own account");

        seed_opening_chart(&graph, &scope(), "personal").expect("the rest of the chart");

        let cash = Records::new(&graph, LEDGER_ACCOUNT)
            .list(&scope(), 100)
            .expect("accounts")
            .into_iter()
            .find(|row| text(row, "account_number") == "1000")
            .expect("cash");
        assert_eq!(
            text(&cash, "name"),
            "Petty Cash",
            "seeding must not rename an account somebody named"
        );
    }

    #[test]
    fn a_new_book_can_be_written_in_without_being_configured_first() {
        let graph = MemoryGraph::new();
        seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        // the sale from the first-run walk, posted against the opening chart rather than a hand-made one
        let entry = json!({
            "entry_date": "2026-11-15",
            "description": "first sale",
            "status": "posted",
            "lines": [
                {"account_number": "1000", "debit": "250.00", "credit": "0"},
                {"account_number": "4000", "debit": "0", "credit": "250.00"},
            ],
        });
        let posted = crate::ledger_book::create_entry(&graph, &scope(), &entry).expect("an entry");
        assert_eq!(text(&posted, "status"), "posted");

        let balance = crate::ledger_book::trial_balance(&graph, &scope(), &json!({}))
            .expect("a trial balance");
        assert_eq!(balance["total_debits"], json!("250.000000"), "{balance}");
        assert_eq!(balance["is_balanced"], json!(true), "{balance}");
    }
}
