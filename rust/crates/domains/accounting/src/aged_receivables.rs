//! Who owes the business, how late they are, and what to send them.
//!
//! The receivables ledger ages invoices from their own date and reports the whole invoice as still owed, because the
//! service it came from never applied a receipt to a document. A person chasing money needs the opposite: what is
//! still owed on each invoice after the money that actually arrived, measured from the day it fell due, and a sentence
//! they can send.
//!
//! The rules, as this reads them:
//!
//! * an invoice counts once it is dated on or before the day being aged to, and not if it is void or cancelled,
//! * a receipt counts once it is dated on or before that day,
//! * a receipt that names a document pays that document; what is left of it pays that customer's oldest invoice first,
//!   which is what a person does when the money arrives without a note,
//! * an invoice with no due date is aged from its own date, and the item says so rather than guessing a term,
//! * a customer who has paid more than was invoiced is shown in credit rather than netted away, so their money is not
//!   quietly absorbed by another customer's debt,
//! * a document whose date cannot be read is counted and named as not aged, never dropped silently,
//! * the day to age from is the one asked for; with none asked for the newest date the Book holds is used, and the
//!   answer says which day it used and why.

use serde_json::{json, Value};

use crate::ageing::{band, columns, is_void, least, readable, BANDS};
use vimbai_common::dates::days_between;
use vimbai_common::Money;

use vimbai_store::amounts::{amount, stored_amount, wire};
use vimbai_store::read::{field, optional};
use vimbai_store::{BookScope, Graph, Records, StoreError, CASH_RECEIPTS_JOURNAL, SALES_JOURNAL};

/// One invoice, and what is still owed on it.
struct Open {
    customer: String,
    /// The customer's own name where the document carries one, so a person reads a name rather than an id.
    name: String,
    number: String,
    invoice_date: String,
    /// The day the debt fell due: its due date, or its own date when it carries none.
    basis: String,
    /// Whether that day is the invoice's own date because no due date was recorded.
    from_own_date: bool,
    amount: Money,
    paid: Money,
}

impl Open {
    fn outstanding(&self) -> Money {
        self.amount.minus(&self.paid)
    }

    fn display(&self) -> String {
        if self.name.trim().is_empty() {
            self.customer.clone()
        } else {
            self.name.clone()
        }
    }
}

/// `GET /aged-receivables`: who owes what, how late, and a sentence to send them.
pub fn aged_receivables<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let invoices = Records::new(graph, SALES_JOURNAL).list(scope, 5000)?;
    let receipts = Records::new(graph, CASH_RECEIPTS_JOURNAL).list(scope, 5000)?;

    let asked = optional(query, "as_of_date").or_else(|| optional(query, "as_of"));
    let (as_of, aged_from) = match asked
        .as_deref()
        .map(str::trim)
        .filter(|day| !day.is_empty())
    {
        Some(day) => {
            if !readable(day) {
                return Err(StoreError::Invalid(format!(
                    "`{day}` is not a day written as YYYY-MM-DD, so there is no day to age to"
                )));
            }
            (day.to_string(), "the day you asked for")
        }
        None => {
            let newest = invoices
                .iter()
                .map(|row| field(row, "invoice_date").to_string())
                .chain(
                    receipts
                        .iter()
                        .map(|row| field(row, "receipt_date").to_string()),
                )
                .filter(|date| readable(date))
                .max();
            match newest {
                Some(day) => (day, "the newest date this Book holds"),
                None => {
                    return Err(StoreError::Invalid(
                        "this Book holds no dated invoice or receipt, so name the day to age to with `as_of_date`"
                            .into(),
                    ))
                }
            }
        }
    };

    let mut undated = 0;
    let mut open: Vec<Open> = Vec::new();
    for row in &invoices {
        if is_void(field(row, "status")) {
            continue;
        }
        let invoice_date = field(row, "invoice_date").to_string();
        match days_between(&as_of, &invoice_date) {
            // raised after the day being aged to, so it is not owed on that day
            Some(signed) if signed < 0 => continue,
            Some(_) => {}
            None => {
                undated += 1;
                continue;
            }
        }
        let due = field(row, "due_date").trim().to_string();
        let due_readable = !due.is_empty() && readable(&due);
        open.push(Open {
            customer: field(row, "customer_id").to_string(),
            name: field(row, "customer_name").to_string(),
            number: field(row, "invoice_number").to_string(),
            basis: if due_readable {
                due.clone()
            } else {
                invoice_date.clone()
            },
            invoice_date,
            from_own_date: !due_readable,
            amount: amount(&json!({"amount": field(row, "total_amount")}), "amount")?,
            paid: Money::zero(),
        });
    }
    // oldest first: an unmarked receipt pays the oldest debt, and the answer lists debts in the order a person would
    // chase them
    open.sort_by(|left, right| {
        left.invoice_date
            .cmp(&right.invoice_date)
            .then_with(|| left.number.cmp(&right.number))
    });

    let mut credits: Vec<(String, Money)> = Vec::new();
    for row in &receipts {
        if is_void(field(row, "status")) {
            continue;
        }
        let receipt_date = field(row, "receipt_date").to_string();
        match days_between(&as_of, &receipt_date) {
            Some(signed) if signed >= 0 => {}
            _ => {
                undated += 1;
                continue;
            }
        }
        let customer = field(row, "customer_id").to_string();
        let document = field(row, "document_reference").trim().to_string();
        let mut left = stored_amount(row, "amount")?;

        if !document.is_empty() {
            if let Some(target) = open.iter_mut().find(|invoice| {
                invoice.number.eq_ignore_ascii_case(&document)
                    && (customer.is_empty() || invoice.customer == customer)
            }) {
                let applied = least(&left, &target.outstanding());
                target.paid = target.paid.plus(&applied);
                left = left.minus(&applied);
            }
        }
        if left.is_zero() || customer.is_empty() {
            continue;
        }
        for invoice in open
            .iter_mut()
            .filter(|invoice| invoice.customer == customer)
        {
            if left.is_zero() {
                break;
            }
            let room = invoice.outstanding();
            if room.is_negative() || room.is_zero() {
                continue;
            }
            let applied = least(&left, &room);
            invoice.paid = invoice.paid.plus(&applied);
            left = left.minus(&applied);
        }
        if !left.is_zero() {
            match credits.iter_mut().find(|(name, _)| name == &customer) {
                Some((_, held)) => *held = held.plus(&left),
                None => credits.push((customer, left)),
            }
        }
    }

    // an invoice paid past its own amount is credit too, rather than a negative debt hiding in a band
    for invoice in &open {
        let out = invoice.outstanding();
        if out.is_negative() {
            match credits
                .iter_mut()
                .find(|(name, _)| name == &invoice.customer)
            {
                Some((_, held)) => *held = held.plus(&out.abs()),
                None => credits.push((invoice.customer.clone(), out.abs())),
            }
        }
    }

    let mut names: Vec<String> = open.iter().map(|row| row.customer.clone()).collect();
    names.sort();
    names.dedup();

    let mut rows: Vec<Value> = Vec::new();
    let mut owed_all = Money::zero();
    let mut late_all = Money::zero();
    let mut credit_all = Money::zero();
    let mut oldest_late = 0;
    for name in &names {
        let mine: Vec<&Open> = open.iter().filter(|row| row.customer == *name).collect();
        let credit = credits
            .iter()
            .find(|(who, _)| who == name)
            .map(|(_, held)| held.clone())
            .unwrap_or_else(Money::zero);
        let mut owed = Money::zero();
        let mut late = Money::zero();
        let mut not_due = Money::zero();
        let mut bands: Vec<(&'static str, Money)> =
            BANDS.iter().map(|band| (*band, Money::zero())).collect();
        let mut items: Vec<Value> = Vec::new();
        let mut worst: Option<(i64, &Open, Money)> = None;
        let mut soonest: Option<(i64, &Open)> = None;

        for invoice in &mine {
            let outstanding = invoice.outstanding();
            if outstanding.is_negative() || outstanding.is_zero() {
                continue;
            }
            let days_late = days_between(&as_of, &invoice.basis).unwrap_or(0);
            owed = owed.plus(&outstanding);
            match band(days_late) {
                Some(key) => {
                    late = late.plus(&outstanding);
                    if let Some(slot) = bands.iter_mut().find(|(name, _)| *name == key) {
                        slot.1 = slot.1.plus(&outstanding);
                    }
                    if worst
                        .as_ref()
                        .map(|(days, _, _)| days_late > *days)
                        .unwrap_or(true)
                    {
                        worst = Some((days_late, invoice, outstanding.clone()));
                    }
                }
                None => {
                    not_due = not_due.plus(&outstanding);
                    if soonest
                        .as_ref()
                        .map(|(days, _)| days_late < *days)
                        .unwrap_or(true)
                    {
                        soonest = Some((days_late, invoice));
                    }
                }
            }
            items.push(json!({
                "invoice_number": invoice.number,
                "invoice_date": invoice.invoice_date,
                "due_date": if invoice.from_own_date { Value::Null } else { json!(invoice.basis) },
                "aged_from": if invoice.from_own_date {
                    "its own date, because no due date was recorded"
                } else {
                    "its due date"
                },
                "invoice_amount": wire(&invoice.amount),
                "paid": wire(&invoice.paid),
                "outstanding": wire(&outstanding),
                "days_late": if days_late > 0 { json!(days_late) } else { Value::Null },
                "band": band(days_late).unwrap_or("not_due"),
            }));
        }

        let display = mine
            .first()
            .map(|row| row.display())
            .unwrap_or_else(|| name.clone());
        let reminder = match (&worst, &soonest) {
            (Some((days, invoice, amount)), _) => format!(
                "{display} owes {owed}. The oldest unpaid invoice, {} for {}, fell due on {} and is {days} days late.",
                invoice.number,
                wire(amount),
                invoice.basis
            ),
            (None, Some((_, invoice))) if !owed.is_zero() => format!(
                "{display} owes {owed} and nothing is late yet; the earliest falls due on {}.",
                invoice.basis
            ),
            (None, None) if !credit.is_zero() => format!(
                "{display} has paid {} more than was invoiced, so nothing is owed.",
                wire(&credit)
            ),
            _ => format!("{display} owes {owed}."),
        };

        owed_all = owed_all.plus(&owed);
        late_all = late_all.plus(&late);
        credit_all = credit_all.plus(&credit);
        if let Some((days, _, _)) = &worst {
            oldest_late = oldest_late.max(*days);
        }

        // a customer who owes nothing and holds no credit is not a person to chase, so they are not listed
        if owed.is_zero() && credit.is_zero() {
            continue;
        }
        let mut row = json!({
            "customer_id": name,
            "customer_name": display,
            "owed": wire(&owed),
            "late": wire(&late),
            "not_due": wire(&not_due),
            "in_credit": wire(&credit),
            "invoices": items,
            "reminder": reminder,
        });
        // the age of the debt, one column per band, built from the bands themselves so a band cannot go missing
        if let Value::Object(fields) = row {
            let aged = columns(&bands);
            row = Value::Object(fields.into_iter().chain(aged).collect());
        }
        rows.push(row);
    }
    rows.sort_by(|left, right| {
        let owed = |row: &Value| row["owed"].as_str().unwrap_or("0").to_string();
        let left_owed = Money::parse(&owed(left)).unwrap_or_else(|_| Money::zero());
        let right_owed = Money::parse(&owed(right)).unwrap_or_else(|_| Money::zero());
        right_owed
            .as_decimal()
            .cmp(left_owed.as_decimal())
            .then_with(|| {
                left["customer_name"]
                    .as_str()
                    .cmp(&right["customer_name"].as_str())
            })
    });

    let count = rows.len();
    let summary = if count == 0 {
        format!("nothing is owed as of {as_of}: this Book holds no unpaid invoice on that day")
    } else if late_all.is_zero() {
        format!(
            "as of {as_of}, {count} customers owe {} and none of it is late yet",
            wire(&owed_all)
        )
    } else {
        format!(
            "as of {as_of}, {count} customers owe {}, of which {} is late; the oldest debt is {oldest_late} days old",
            wire(&owed_all),
            wire(&late_all)
        )
    };

    Ok(json!({
        "as_of": as_of,
        "aged_from": aged_from,
        "totals": {
            "owed": wire(&owed_all),
            "late": wire(&late_all),
            "in_credit": wire(&credit_all),
            "customers": count,
            "oldest_late_days": oldest_late,
        },
        "customers": rows,
        "not_aged": undated,
        "summary": summary,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::seed_opening_chart;
    use crate::ledger_book::create_account;
    use crate::subsidiary_journals::{create_receipt, create_sale};
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn book() -> MemoryGraph {
        let graph = MemoryGraph::new();
        seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        create_account(
            &graph,
            &scope(),
            &json!({"account_number": "4100", "name": "Other income", "account_type": "revenue",
                    "normal_balance": "credit"}),
        )
        .expect("the account a receipt posts to");
        graph
    }

    fn invoice(
        graph: &MemoryGraph,
        number: &str,
        customer: &str,
        date: &str,
        due: &str,
        total: &str,
    ) {
        create_sale(
            graph,
            &scope(),
            &json!({"invoice_number": number, "customer_id": customer, "customer_name": format!("{customer} Ltd"),
                    "entry_date": date, "due_date": due, "total_amount": total, "tax_amount": "0",
                    "status": "posted"}),
        )
        .expect("an invoice");
    }

    fn receipt(
        graph: &MemoryGraph,
        number: &str,
        customer: &str,
        date: &str,
        total: &str,
        against: &str,
    ) {
        // a receipt moves money through an account the Book keeps, so the Book that takes receipts has one
        if crate::cashbook::list_accounts(graph, &scope(), &json!({"is_active": true}))
            .expect("the Book's accounts")
            .as_array()
            .map(Vec::is_empty)
            .unwrap_or(true)
        {
            crate::cashbook::create_account(
                graph,
                &scope(),
                &json!({"account_code": "CASH-1", "account_name": "Cash", "account_type": "cash",
                        "opening_balance": "0.00", "currency": "USD"}),
            )
            .expect("a cash account");
        }
        create_receipt(
            graph,
            &scope(),
            &json!({"receipt_number": number, "customer_id": customer, "entry_date": date, "amount": total,
                    "document_reference": against, "status": "posted"}),
        )
        .expect("a receipt");
    }

    fn aged(graph: &MemoryGraph, day: &str) -> Value {
        aged_receivables(graph, &scope(), &json!({"as_of_date": day})).expect("an ageing")
    }

    fn customer<'a>(answer: &'a Value, id: &str) -> &'a Value {
        answer["customers"]
            .as_array()
            .expect("customers")
            .iter()
            .find(|row| row["customer_id"] == json!(id))
            .unwrap_or_else(|| panic!("no customer {id} in {answer}"))
    }

    #[test]
    fn an_invoice_past_its_due_date_is_late_and_says_how_late() {
        let graph = book();
        invoice(
            &graph,
            "INV-1",
            "acme",
            "2026-10-01",
            "2026-10-31",
            "500.00",
        );
        let answer = aged(&graph, "2026-11-25");
        let acme = customer(&answer, "acme");
        assert_eq!(acme["owed"], json!("500.000000"));
        assert_eq!(acme["late"], json!("500.000000"));
        assert_eq!(acme["1_30"], json!("500.000000"));
        assert_eq!(acme["invoices"][0]["days_late"], json!(25));
        assert_eq!(acme["invoices"][0]["band"], json!("1_30"));
        assert_eq!(answer["totals"]["oldest_late_days"], json!(25));
        // and the sentence a person can send names the invoice, the amount and the day
        let reminder = acme["reminder"].as_str().unwrap_or_default();
        assert!(reminder.contains("INV-1"), "{reminder}");
        assert!(reminder.contains("500.000000"), "{reminder}");
        assert!(reminder.contains("2026-10-31"), "{reminder}");
        assert!(reminder.contains("25 days late"), "{reminder}");
    }

    #[test]
    fn an_invoice_not_yet_due_is_owed_but_not_late() {
        let graph = book();
        invoice(
            &graph,
            "INV-1",
            "acme",
            "2026-11-20",
            "2026-12-20",
            "500.00",
        );
        let answer = aged(&graph, "2026-11-25");
        let acme = customer(&answer, "acme");
        assert_eq!(acme["owed"], json!("500.000000"));
        assert_eq!(acme["late"], json!("0.000000"));
        assert_eq!(acme["not_due"], json!("500.000000"));
        assert_eq!(acme["invoices"][0]["band"], json!("not_due"));
        assert!(acme["reminder"]
            .as_str()
            .unwrap_or_default()
            .contains("nothing is late"));
    }

    #[test]
    fn a_receipt_that_names_the_invoice_clears_it() {
        let graph = book();
        invoice(
            &graph,
            "INV-1",
            "acme",
            "2026-10-01",
            "2026-10-31",
            "500.00",
        );
        receipt(&graph, "RCT-1", "acme", "2026-11-02", "500.00", "INV-1");
        let answer = aged(&graph, "2026-11-25");
        assert_eq!(answer["totals"]["owed"], json!("0.000000"));
        assert_eq!(answer["totals"]["customers"], json!(0));
        assert!(answer["summary"]
            .as_str()
            .unwrap_or_default()
            .contains("nothing is owed"));
    }

    #[test]
    fn a_receipt_with_no_note_pays_the_oldest_invoice_first() {
        let graph = book();
        invoice(
            &graph,
            "INV-1",
            "acme",
            "2026-09-01",
            "2026-09-30",
            "300.00",
        );
        invoice(
            &graph,
            "INV-2",
            "acme",
            "2026-10-01",
            "2026-10-31",
            "400.00",
        );
        receipt(&graph, "RCT-1", "acme", "2026-11-10", "300.00", "");
        let answer = aged(&graph, "2026-11-25");
        let acme = customer(&answer, "acme");
        // the money cleared the older debt, and the newer one is what is left
        assert_eq!(acme["owed"], json!("400.000000"));
        assert_eq!(acme["invoices"].as_array().map(Vec::len), Some(1));
        assert_eq!(acme["invoices"][0]["invoice_number"], json!("INV-2"));
    }

    #[test]
    fn part_of_an_invoice_paid_leaves_the_rest_owed() {
        let graph = book();
        invoice(
            &graph,
            "INV-1",
            "acme",
            "2026-10-01",
            "2026-10-31",
            "500.00",
        );
        receipt(&graph, "RCT-1", "acme", "2026-11-02", "200.00", "INV-1");
        let answer = aged(&graph, "2026-11-25");
        let item = &customer(&answer, "acme")["invoices"][0];
        assert_eq!(item["invoice_amount"], json!("500.000000"));
        assert_eq!(item["paid"], json!("200.000000"));
        assert_eq!(item["outstanding"], json!("300.000000"));
    }

    #[test]
    fn an_invoice_with_no_due_date_is_aged_from_its_own_date_and_says_so() {
        let graph = book();
        invoice(&graph, "INV-1", "acme", "2026-10-01", "", "500.00");
        let answer = aged(&graph, "2026-11-25");
        let item = &customer(&answer, "acme")["invoices"][0];
        assert_eq!(item["due_date"], Value::Null);
        assert_eq!(
            item["aged_from"],
            json!("its own date, because no due date was recorded")
        );
        assert_eq!(item["days_late"], json!(55));
    }

    #[test]
    fn money_that_arrived_after_the_day_is_not_counted_on_that_day() {
        let graph = book();
        invoice(
            &graph,
            "INV-1",
            "acme",
            "2026-10-01",
            "2026-10-31",
            "500.00",
        );
        receipt(&graph, "RCT-1", "acme", "2026-12-05", "500.00", "INV-1");
        let answer = aged(&graph, "2026-11-25");
        assert_eq!(customer(&answer, "acme")["owed"], json!("500.000000"));
    }

    #[test]
    fn an_invoice_raised_after_the_day_is_not_owed_on_that_day() {
        let graph = book();
        invoice(
            &graph,
            "INV-1",
            "acme",
            "2026-12-01",
            "2026-12-31",
            "500.00",
        );
        let answer = aged(&graph, "2026-11-25");
        assert_eq!(answer["totals"]["customers"], json!(0));
    }

    #[test]
    fn paying_more_than_was_invoiced_is_shown_as_credit_rather_than_a_negative_debt() {
        let graph = book();
        invoice(
            &graph,
            "INV-1",
            "acme",
            "2026-10-01",
            "2026-10-31",
            "500.00",
        );
        receipt(&graph, "RCT-1", "acme", "2026-11-02", "620.00", "INV-1");
        let answer = aged(&graph, "2026-11-25");
        let acme = customer(&answer, "acme");
        assert_eq!(acme["owed"], json!("0.000000"));
        assert_eq!(acme["in_credit"], json!("120.000000"));
        assert_eq!(answer["totals"]["in_credit"], json!("120.000000"));
        assert!(acme["reminder"]
            .as_str()
            .unwrap_or_default()
            .contains("120.000000"));
    }

    #[test]
    fn four_invoices_split_across_the_four_bands() {
        let graph = book();
        // aged to 2026-11-25: 10, 45, 75 and 120 days past the day each fell due
        invoice(
            &graph,
            "INV-1",
            "acme",
            "2026-10-01",
            "2026-11-15",
            "100.00",
        );
        invoice(
            &graph,
            "INV-2",
            "acme",
            "2026-09-01",
            "2026-10-11",
            "200.00",
        );
        invoice(
            &graph,
            "INV-3",
            "acme",
            "2026-08-01",
            "2026-09-11",
            "300.00",
        );
        invoice(
            &graph,
            "INV-4",
            "acme",
            "2026-07-01",
            "2026-07-28",
            "400.00",
        );
        let answer = aged(&graph, "2026-11-25");
        let acme = customer(&answer, "acme");
        assert_eq!(acme["1_30"], json!("100.000000"));
        assert_eq!(acme["31_60"], json!("200.000000"));
        assert_eq!(acme["61_90"], json!("300.000000"));
        assert_eq!(acme["over_90"], json!("400.000000"));
        assert_eq!(acme["late"], json!("1000.000000"));
        assert_eq!(acme["owed"], json!("1000.000000"));
    }

    #[test]
    fn a_voided_invoice_is_not_owed() {
        let graph = book();
        create_sale(
            &graph,
            &scope(),
            &json!({"invoice_number": "INV-1", "customer_id": "acme", "invoice_date": "2026-10-01",
                    "due_date": "2026-10-31", "total_amount": "500.00", "tax_amount": "0", "status": "void"}),
        )
        .expect("a voided invoice");
        let answer = aged(&graph, "2026-11-25");
        assert_eq!(answer["totals"]["customers"], json!(0));
    }

    #[test]
    fn a_day_that_cannot_be_read_is_refused_naming_it() {
        let graph = book();
        invoice(
            &graph,
            "INV-1",
            "acme",
            "2026-10-01",
            "2026-10-31",
            "500.00",
        );
        let refused = aged_receivables(&graph, &scope(), &json!({"as_of_date": "25 November"}))
            .expect_err("an unreadable day");
        assert!(refused.to_string().contains("`25 November`"), "{refused}");
    }

    #[test]
    fn with_no_day_asked_for_the_newest_date_the_book_holds_is_used_and_named() {
        let graph = book();
        invoice(
            &graph,
            "INV-1",
            "acme",
            "2026-10-01",
            "2026-10-31",
            "500.00",
        );
        receipt(&graph, "RCT-1", "acme", "2026-11-09", "100.00", "INV-1");
        let answer = aged_receivables(&graph, &scope(), &json!({})).expect("an ageing");
        assert_eq!(answer["as_of"], json!("2026-11-09"));
        assert_eq!(
            answer["aged_from"],
            json!("the newest date this Book holds")
        );
    }

    #[test]
    fn a_customer_who_owes_most_is_listed_first() {
        let graph = book();
        invoice(
            &graph,
            "INV-1",
            "acme",
            "2026-10-01",
            "2026-10-31",
            "100.00",
        );
        invoice(
            &graph,
            "INV-2",
            "zenith",
            "2026-10-01",
            "2026-10-31",
            "900.00",
        );
        let answer = aged(&graph, "2026-11-25");
        assert_eq!(answer["customers"][0]["customer_id"], json!("zenith"));
        assert_eq!(answer["customers"][1]["customer_id"], json!("acme"));
        assert_eq!(answer["totals"]["owed"], json!("1000.000000"));
    }
}
