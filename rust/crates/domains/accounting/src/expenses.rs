//! The general expenses journal: what the business spent running itself, and who it was paid to.
//!
//! A Book records plenty of money going out against a supplier of stock - a purchase, a bill, a payment - and nothing for
//! the ordinary costs of being open: rent, fuel, airtime, wages, bank charges, a licence. Those are not purchases of
//! anything held for sale and they are not a supplier's bill, so they had no journal of their own and every one of them had
//! to be hand-written as a general entry or squeezed through a cash book payment against a code somebody had to remember.
//!
//! What this journal keeps:
//!
//! * the figure asked for is what was actually spent, tax included, because that is what the receipt says. The tax inside
//!   it comes out of that figure rather than being added on top, exactly as it does on a bill, and each tax goes to its own
//!   account because each is reclaimed separately;
//! * an expense that was paid says which cash or bank account it left. The money the business holds moves in the cash book
//!   in the same act, so the cash book and the ledger never disagree about the same payment. Vimbai does not move the money:
//!   it records what already left;
//! * an expense that has not been paid yet is owed instead, so it credits payables and touches no cash at all. Recording a
//!   cost when it is incurred is the whole point of an accrual, and it is not the same event as paying it;
//! * a cost in another money is carried into the Book's own at the rate stated, and the document keeps what it said;
//! * the reference is the person's own, so the same expense cannot be recorded twice: the ledger refuses a reference it has
//!   already posted.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, money, money_or_nothing, optional, or_default, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, EXPENSES_JOURNAL};

use crate::cashbook;
use crate::subsidiary_journals::{
    check_tax_fits, post, post_taxed, post_taxed_lines, tax_from_total, TaxedDocument,
};

/// Where the money left, as the cash book knows the account.
///
/// The cash book's own code and the ledger's account number are different things: one names a bank account the business
/// holds, the other the account in the chart that money sits in. A payment needs both, so both are asked for rather than
/// guessed from a name.
struct MoneyLeft {
    /// The cash book account code, whose balance moves.
    held_in: String,
    /// The chart account the money sits in, which the ledger credits.
    ledger_account: String,
}

/// What left the account, in the money it left in: the document's own figure when it was raised abroad, else the amount.
fn cashbook_amount(payload: &Value, total: &Money) -> String {
    let stated = text(payload, "document_total");
    if stated.trim().is_empty() {
        total.to_fixed(6)
    } else {
        stated
    }
}

fn money_left(payload: &Value) -> Result<Option<MoneyLeft>, StoreError> {
    let held_in = text(payload, "paid_from");
    if held_in.trim().is_empty() {
        return Ok(None);
    }
    let ledger_account = text(payload, "money_account");
    if ledger_account.trim().is_empty() {
        return Err(refused(
            "Say which account in the chart the money sits in, so the payment can be posted against it as well as \
             recorded in the cash book.",
        ));
    }
    Ok(Some(MoneyLeft {
        held_in: held_in.trim().to_string(),
        ledger_account: ledger_account.trim().to_string(),
    }))
}

/// `POST /expenses-journal/`: a general expense, paid or still owed.
pub fn create_expense(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    // a cost in another money is carried into the Book's own before a single line is posted, so the ledger never adds
    // dollars to bond notes; the document keeps what it actually said
    let (converted, document) = crate::document_currency::in_book_money(graph, scope, payload)?;
    let payload = &converted;
    let _ = &document;

    let total = money(payload, "total_amount")?;
    if total.is_zero() || total.is_negative() {
        return Err(refused(
            "An expense has to be more than nothing: say what was actually spent.",
        ));
    }
    let reference = text(payload, "reference_number");
    if reference.trim().is_empty() {
        return Err(refused(
            "Give the expense a reference of your own, so the same one cannot be recorded twice.",
        ));
    }
    let expense_account = text(payload, "expense_account");
    if expense_account.trim().is_empty() {
        return Err(refused(
            "Say which account the cost belongs in, so it shows in the right part of the accounts.",
        ));
    }

    // the tax inside the figure, taken out of it rather than added to it, one line per tax on its own account
    let typed_tax = money_or_nothing(payload, "tax_amount")?;
    let (tax, tax_lines) = if typed_tax.is_zero() {
        tax_from_total(graph, scope, payload, false, &total)?
    } else {
        (typed_tax, Vec::new())
    };
    check_tax_fits(&tax, &total, &format!("expense {reference}"))?;

    let paid = money_left(payload)?;
    // owed unless it says it was paid: a cost that has not left the account yet is a liability, not a payment
    let credit_account = match &paid {
        Some(money) => money.ledger_account.clone(),
        None => crate::subsidiary_journals::account_for_purpose(
            graph,
            scope,
            &text(payload, "payable_account"),
            "payable",
            "liability",
        )?,
    };
    let payee = text(payload, "paid_to");
    let note = if payee.trim().is_empty() {
        format!("Expense {reference}")
    } else {
        format!("Expense {reference} paid to {payee}")
    };

    let entry_id = if tax.is_zero() {
        post(
            graph,
            scope,
            payload,
            format!("Expenses Journal: {reference}"),
            &reference,
            "ExpensesJournal",
            &expense_account,
            &credit_account,
            &total,
            &note,
        )?
    } else if !tax_lines.is_empty() {
        post_taxed_lines(
            graph,
            scope,
            payload,
            format!("Expenses Journal: {reference}"),
            &reference,
            "ExpensesJournal",
            TaxedDocument::Expense,
            &expense_account,
            &credit_account,
            &total,
            &tax_lines,
            &note,
        )?
    } else {
        post_taxed(
            graph,
            scope,
            payload,
            format!("Expenses Journal: {reference}"),
            &reference,
            "ExpensesJournal",
            TaxedDocument::Expense,
            &expense_account,
            &credit_account,
            &total,
            &tax,
            &note,
        )?
    };

    // money that already left is recorded where the money the business holds is kept, in the same act as its double entry,
    // so the cash book cannot end up believing the bank still has money the ledger has already spent
    let cash_entry_id = match &paid {
        Some(money) => {
            let cash = cashbook::create_entry(
                graph,
                scope,
                &json!({
                    "bank_account": money.held_in,
                    "account_code": expense_account,
                    "entry_date": text(payload, "entry_date"),
                    "voucher_number": reference,
                    "description": if payee.trim().is_empty() {
                        format!("Expense {reference}")
                    } else {
                        format!("Expense {reference} paid to {payee}")
                    },
                    // the figure that left the account, in the money it left in, exactly as it was spent
                    "amount": cashbook_amount(payload, &total),
                    "currency": or_default(payload, "document_currency", "USD"),
                    "exchange_rate": or_default(payload, "document_rate", "1"),
                    "is_debit": false,
                    "book_type": "payments",
                    "status": "posted",
                }),
            )?;
            Some(field(&cash, "id").to_string())
        }
        None => None,
    };

    remember(
        graph,
        scope,
        EXPENSES_JOURNAL,
        json!({
            "reference_number": reference,
            "expense_account": expense_account,
            "paid_to": payee,
            "expense_date": text(payload, "entry_date"),
            "total_amount": wire(&total),
            "tax_amount": wire(&tax),
            "paid_from": paid.as_ref().map(|money| money.held_in.clone()),
            "money_account": paid.as_ref().map(|money| money.ledger_account.clone()),
            "payable_account": if paid.is_none() { credit_account } else { String::new() },
            "description": text(payload, "description"),
            "document_currency": text(payload, "document_currency"),
            "document_rate": text(payload, "document_rate"),
            "document_total": text(payload, "document_total"),
            "notes": text(payload, "notes"),
            "journal_entry_id": entry_id,
            "cash_entry_id": cash_entry_id,
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /expenses-journal/`: what the Book has recorded, newest first.
pub fn list_expenses(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let start = optional(query, "start_date");
    let end = optional(query, "end_date");
    let account = optional(query, "expense_account");
    let payee = optional(query, "paid_to");
    let limit = vimbai_store::query::query_page(query, 100, 1000);

    let mut kept: Vec<Value> = Records::new(graph, EXPENSES_JOURNAL)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| {
            start
                .as_deref()
                .map(|want| field(row, "expense_date") >= want)
                .unwrap_or(true)
                && end
                    .as_deref()
                    .map(|want| field(row, "expense_date") <= want)
                    .unwrap_or(true)
                && account
                    .as_deref()
                    .map(|want| field(row, "expense_account") == want)
                    .unwrap_or(true)
                && payee
                    .as_deref()
                    .map(|want| {
                        field(row, "paid_to")
                            .to_lowercase()
                            .contains(&want.to_lowercase())
                    })
                    .unwrap_or(true)
        })
        .collect();
    kept.sort_by(|left, right| field(right, "expense_date").cmp(field(left, "expense_date")));
    kept.truncate(limit);
    Ok(Value::Array(kept))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    /// A business Book with the chart a business opens with.
    fn business() -> (MemoryGraph, BookScope) {
        let graph = MemoryGraph::new();
        let scope = scope();
        crate::chart::seed_opening_chart(&graph, &scope, "business").expect("a chart");
        (graph, scope)
    }

    /// The line on an account, read back from the entry that was posted.
    fn line(graph: &MemoryGraph, scope: &BookScope, entry_id: &str, account: &str) -> Value {
        let entry = crate::ledger_book::get_entry(graph, scope, entry_id).expect("the entry");
        entry["lines"]
            .as_array()
            .cloned()
            .expect("lines")
            .into_iter()
            .find(|one| field(one, "account_number") == account)
            .unwrap_or_else(|| panic!("no line on {account}"))
    }

    /// A bank account the business holds, with money already in it.
    fn bank(graph: &MemoryGraph, scope: &BookScope) -> Value {
        cashbook::create_account(
            graph,
            scope,
            &json!({"account_code": "BANK-1", "account_name": "Bank", "account_type": "bank",
                    "opening_balance": "5000.00"}),
        )
        .expect("a bank account")
    }

    /// An expense that has not been paid is a cost now and a liability until it is: it must not touch cash at all.
    #[test]
    fn an_expense_still_owed_credits_payables_and_touches_no_cash() {
        let (graph, scope) = business();
        let posted = create_expense(
            &graph,
            &scope,
            &json!({"entry_date": "2026-11-24", "reference_number": "EXP-1",
                    "expense_account": "6100", "paid_to": "Landlord",
                    "total_amount": "1500.00", "description": "November rent"}),
        )
        .expect("recorded");

        let entry_id = field(&posted, "journal_entry_id").to_string();
        assert_eq!(
            money(&line(&graph, &scope, &entry_id, "6100"), "debit").expect("the rent"),
            Money::parse("1500.00").expect("1,500")
        );
        assert_eq!(
            money(&line(&graph, &scope, &entry_id, "2000"), "credit").expect("owed"),
            Money::parse("1500.00").expect("1,500")
        );
        // nothing left the bank, so the cash book has no entry for it
        assert!(posted["cash_entry_id"].is_null());
        assert!(cashbook::list_entries(&graph, &scope, &json!({}))
            .expect("entries")
            .as_array()
            .expect("a list")
            .is_empty());
    }

    /// An expense that was paid moves the money the business holds in the same act as its double entry, so the cash book
    /// and the ledger cannot end up disagreeing about the same payment.
    #[test]
    fn an_expense_already_paid_moves_the_cash_book_and_the_ledger_together() {
        let (graph, scope) = business();
        bank(&graph, &scope);
        let posted = create_expense(
            &graph,
            &scope,
            &json!({"entry_date": "2026-11-24", "reference_number": "EXP-2",
                    "expense_account": "6300", "paid_to": "Stationer",
                    "total_amount": "300.00", "paid_from": "BANK-1", "money_account": "1010"}),
        )
        .expect("recorded");

        let entry_id = field(&posted, "journal_entry_id").to_string();
        assert_eq!(
            money(&line(&graph, &scope, &entry_id, "6300"), "debit").expect("the cost"),
            Money::parse("300.00").expect("300")
        );
        assert_eq!(
            money(&line(&graph, &scope, &entry_id, "1010"), "credit").expect("the money"),
            Money::parse("300.00").expect("300")
        );

        // the bank the business holds is 300 lighter, and the cash book says so
        let accounts = cashbook::list_accounts(&graph, &scope, &json!({})).expect("accounts");
        let held = accounts
            .as_array()
            .expect("a list")
            .iter()
            .find(|one| field(one, "account_code") == "BANK-1")
            .expect("the bank account")
            .clone();
        assert_eq!(
            money(&held, "current_balance").expect("the balance"),
            Money::parse("4700.00").expect("4,700")
        );
        let entries = cashbook::list_entries(&graph, &scope, &json!({"bank_account": "BANK-1"}))
            .expect("entries");
        let only = entries
            .as_array()
            .expect("a list")
            .first()
            .expect("one entry");
        assert_eq!(only["is_debit"], json!(false));
        assert_eq!(field(only, "status"), "posted");
        assert_eq!(field(only, "voucher_number"), "EXP-2");
        assert_eq!(field(only, "account_code"), "6300");
    }

    /// The tax inside a cost is reclaimed, so it goes to the tax's own account rather than being folded into the expense.
    #[test]
    fn tax_inside_the_expense_is_reclaimed_on_its_own_account() {
        let (graph, scope) = business();
        let posted = create_expense(
            &graph,
            &scope,
            &json!({"entry_date": "2026-11-24", "reference_number": "EXP-3",
                    "expense_account": "6300", "total_amount": "1150.00",
                    "tax_amount": "150.00", "tax_account_number": "2100"}),
        )
        .expect("recorded");

        let entry_id = field(&posted, "journal_entry_id").to_string();
        assert_eq!(
            money(&line(&graph, &scope, &entry_id, "6300"), "debit").expect("the cost"),
            Money::parse("1000.00").expect("1,000 without its tax")
        );
        assert_eq!(
            money(&line(&graph, &scope, &entry_id, "2100"), "debit").expect("the tax"),
            Money::parse("150.00").expect("150 reclaimed")
        );
        assert_eq!(
            money(&line(&graph, &scope, &entry_id, "2000"), "credit").expect("owed"),
            Money::parse("1150.00").expect("the gross")
        );
    }

    /// A cost in another money is carried into the Book's own, and the document keeps what it actually said.
    #[test]
    fn a_cost_in_another_money_is_carried_into_the_books_money() {
        let (graph, scope) = business();
        vimbai_store::Records::new(&graph, vimbai_store::STANDARDS_CONFIG)
            .create(
                &scope,
                &json!({"organization_id": "org-1", "reporting_currency": "ZWL"}),
            )
            .expect("the Book keeps bond notes");
        bank(&graph, &scope);
        let posted = create_expense(
            &graph,
            &scope,
            &json!({"entry_date": "2026-11-24", "reference_number": "EXP-4",
                    "expense_account": "6300", "total_amount": "100.00",
                    "currency": "USD", "exchange_rate": "25.5",
                    "paid_from": "BANK-1", "money_account": "1010"}),
        )
        .expect("recorded");

        let entry_id = field(&posted, "journal_entry_id").to_string();
        assert_eq!(
            money(&line(&graph, &scope, &entry_id, "6300"), "debit").expect("the cost"),
            Money::parse("2550.00").expect("100 dollars at 25.5")
        );
        // what left the account is what was spent, in the money it was spent in
        let entries = cashbook::list_entries(&graph, &scope, &json!({"bank_account": "BANK-1"}))
            .expect("entries");
        let only = entries
            .as_array()
            .expect("a list")
            .first()
            .expect("one entry");
        assert_eq!(field(only, "amount"), "100.000000");
        assert_eq!(field(only, "currency"), "USD");
        assert_eq!(field(only, "base_amount"), "2550.000000");
    }

    #[test]
    fn an_expense_of_nothing_a_missing_reference_and_a_missing_account_are_all_refused() {
        let (graph, scope) = business();
        let nothing = create_expense(
            &graph,
            &scope,
            &json!({"entry_date": "2026-11-24", "reference_number": "EXP-5",
                    "expense_account": "6300", "total_amount": "0"}),
        )
        .expect_err("nothing spent");
        assert!(format!("{nothing}").contains("more than nothing"));

        let unreferenced = create_expense(
            &graph,
            &scope,
            &json!({"entry_date": "2026-11-24", "expense_account": "6300",
                    "total_amount": "10.00"}),
        )
        .expect_err("no reference");
        assert!(format!("{unreferenced}").contains("reference of your own"));

        let unaccounted = create_expense(
            &graph,
            &scope,
            &json!({"entry_date": "2026-11-24", "reference_number": "EXP-6",
                    "total_amount": "10.00"}),
        )
        .expect_err("no account");
        assert!(format!("{unaccounted}").contains("which account the cost belongs in"));
    }

    /// A payment needs both the account the business holds and the account in the chart: one moves the cash book, the
    /// other is what the ledger credits, and guessing either would post to the wrong place.
    #[test]
    fn a_payment_without_the_chart_account_it_sits_in_is_refused() {
        let (graph, scope) = business();
        bank(&graph, &scope);
        let refused = create_expense(
            &graph,
            &scope,
            &json!({"entry_date": "2026-11-24", "reference_number": "EXP-7",
                    "expense_account": "6300", "total_amount": "300.00",
                    "paid_from": "BANK-1"}),
        )
        .expect_err("no money account");
        assert!(format!("{refused}").contains("which account in the chart"));
    }

    /// Tax bigger than the cost is not a cost with tax in it; it is a figure somebody has mixed up.
    #[test]
    fn tax_bigger_than_the_expense_is_refused() {
        let (graph, scope) = business();
        let refused = create_expense(
            &graph,
            &scope,
            &json!({"entry_date": "2026-11-24", "reference_number": "EXP-8",
                    "expense_account": "6300", "total_amount": "100.00",
                    "tax_amount": "150.00", "tax_account_number": "2100"}),
        )
        .expect_err("tax over the cost");
        assert!(format!("{refused}").contains("more than the"));
    }

    /// The same reference twice is the same expense twice, which the ledger already refuses by reference.
    #[test]
    fn the_same_reference_cannot_be_recorded_twice() {
        let (graph, scope) = business();
        let once = json!({"entry_date": "2026-11-24", "reference_number": "EXP-9",
                          "expense_account": "6300", "total_amount": "50.00"});
        create_expense(&graph, &scope, &once).expect("recorded");
        create_expense(&graph, &scope, &once).expect_err("the same expense again");
    }

    /// What the Book has recorded comes back newest first, and can be narrowed to one account or one payee.
    #[test]
    fn recorded_expenses_are_listed_newest_first_and_can_be_narrowed() {
        let (graph, scope) = business();
        for (day, reference, account, payee) in [
            ("2026-11-01", "EXP-10", "6100", "Landlord"),
            ("2026-11-20", "EXP-11", "6300", "Stationer"),
        ] {
            create_expense(
                &graph,
                &scope,
                &json!({"entry_date": day, "reference_number": reference,
                        "expense_account": account, "paid_to": payee, "total_amount": "100.00"}),
            )
            .expect("recorded");
        }

        let all = list_expenses(&graph, &scope, &json!({})).expect("listed");
        let all = all.as_array().expect("a list");
        assert_eq!(all.len(), 2);
        assert_eq!(field(&all[0], "reference_number"), "EXP-11", "newest first");

        let rent =
            list_expenses(&graph, &scope, &json!({"expense_account": "6100"})).expect("listed");
        assert_eq!(rent.as_array().expect("a list").len(), 1);
        let by_payee =
            list_expenses(&graph, &scope, &json!({"paid_to": "stationer"})).expect("listed");
        assert_eq!(
            field(
                by_payee.as_array().expect("a list").first().expect("one"),
                "reference_number"
            ),
            "EXP-11",
            "a payee is matched without caring about case"
        );
    }
}
