//! What the business owes, to whom, and how late.
//!
//! The mirror of the receivables ageing, on the other side of the Book. A bill counts once it is dated on or before the
//! day being aged to and is not void; a payment counts once it is dated on or before that day; a payment that names a
//! bill pays that bill, and what is left of it pays that supplier's oldest bill first, which is what a person does when
//! the money leaves without a note. A bill with no due date is aged from its own date and says so, a supplier paid more
//! than was billed is shown in credit rather than netted away, and a document whose date cannot be read is counted and
//! named as not aged rather than dropped.
//!
//! This is also what the fact engine reads before it treats money going out as a supplier being paid: the same reader
//! says what is owed, so a fact and this report can never disagree.

use serde_json::{json, Value};
use vimbai_common::dates::days_between;
use vimbai_common::Money;

use vimbai_store::amounts::{amount, stored_amount, wire};
use vimbai_store::read::{field, optional};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, CASH_DISBURSEMENTS_JOURNAL, VENDOR_BILL,
};

/// One bill, and what is still owed on it.
struct Owed {
    vendor: String,
    name: String,
    number: String,
    invoice_date: String,
    /// The day it fell due: its due date, or its own date when it carries none.
    basis: String,
    from_own_date: bool,
    amount: Money,
    paid: Money,
}

impl Owed {
    fn outstanding(&self) -> Money {
        self.amount.minus(&self.paid)
    }

    fn display(&self) -> String {
        if self.name.trim().is_empty() {
            self.vendor.clone()
        } else {
            self.name.clone()
        }
    }
}

fn is_void(status: &str) -> bool {
    matches!(status, "void" | "voided" | "cancelled" | "canceled")
}

fn band(days_late: i64) -> Option<&'static str> {
    match days_late {
        d if d <= 0 => None,
        1..=30 => Some("1_30"),
        31..=60 => Some("31_60"),
        61..=90 => Some("61_90"),
        _ => Some("over_90"),
    }
}

fn at_least(left: &Money, right: &Money) -> bool {
    left.as_decimal() >= right.as_decimal()
}

fn least(left: &Money, right: &Money) -> Money {
    if at_least(right, left) {
        left.clone()
    } else {
        right.clone()
    }
}

fn readable(date: &str) -> bool {
    days_between(date, date).is_some()
}

/// A bill still owed, as the reader hands it on.
pub(crate) struct BillOwed {
    pub(crate) vendor: String,
    pub(crate) display: String,
    pub(crate) number: String,
    pub(crate) outstanding: Money,
}

/// What a Book owes on a day: the bills still open, oldest first, and any money a supplier was paid past what was billed.
pub(crate) struct OwedOnADay {
    pub(crate) bills: Vec<BillOwed>,
    pub(crate) overpaid: Vec<(String, Money)>,
}

/// The bills still owed on [as_of], oldest first, and any money a supplier was paid past what was billed.
///
/// Shared with the fact engine, which must not count a supplier as owed money this reader says has been paid.
pub(crate) fn open_bills<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    as_of: &str,
) -> Result<OwedOnADay, StoreError> {
    let bills = Records::new(graph, VENDOR_BILL).list(scope, 5000)?;
    let payments = Records::new(graph, CASH_DISBURSEMENTS_JOURNAL).list(scope, 5000)?;

    let mut open: Vec<Owed> = Vec::new();
    for row in &bills {
        if is_void(field(row, "status")) {
            continue;
        }
        let raised = field(row, "invoice_date").to_string();
        match days_between(as_of, &raised) {
            // raised after the day being aged to, so it is not owed on that day
            Some(signed) if signed < 0 => continue,
            Some(_) => {}
            None => continue,
        }
        let due = field(row, "due_date").trim().to_string();
        let due_readable = !due.is_empty() && readable(&due);
        open.push(Owed {
            vendor: field(row, "vendor_id").to_string(),
            name: field(row, "vendor_name").to_string(),
            number: field(row, "bill_number").to_string(),
            basis: if due_readable { due } else { raised.clone() },
            invoice_date: raised,
            from_own_date: !due_readable,
            amount: amount(&json!({"amount": field(row, "total_amount")}), "amount")?,
            paid: Money::zero(),
        });
    }
    open.sort_by(|left, right| {
        left.invoice_date
            .cmp(&right.invoice_date)
            .then_with(|| left.number.cmp(&right.number))
    });

    let mut credits: Vec<(String, Money)> = Vec::new();
    for row in &payments {
        if is_void(field(row, "status")) {
            continue;
        }
        let paid_on = field(row, "payment_date").to_string();
        match days_between(as_of, &paid_on) {
            Some(signed) if signed >= 0 => {}
            _ => continue,
        }
        let vendor = field(row, "vendor_id").to_string();
        let document = field(row, "document_reference").trim().to_string();
        let mut left = stored_amount(row, "amount")?;

        if !document.is_empty() {
            if let Some(target) = open.iter_mut().find(|bill| {
                bill.number.eq_ignore_ascii_case(&document)
                    && (vendor.is_empty() || bill.vendor == vendor)
            }) {
                let applied = least(&left, &target.outstanding());
                target.paid = target.paid.plus(&applied);
                left = left.minus(&applied);
            }
        }
        if left.is_zero() || vendor.is_empty() {
            continue;
        }
        for bill in open.iter_mut().filter(|bill| bill.vendor == vendor) {
            if left.is_zero() {
                break;
            }
            let room = bill.outstanding();
            if room.is_negative() || room.is_zero() {
                continue;
            }
            let applied = least(&left, &room);
            bill.paid = bill.paid.plus(&applied);
            left = left.minus(&applied);
        }
        if !left.is_zero() {
            match credits.iter_mut().find(|(name, _)| name == &vendor) {
                Some((_, held)) => *held = held.plus(&left),
                None => credits.push((vendor, left)),
            }
        }
    }
    for bill in &open {
        let out = bill.outstanding();
        if out.is_negative() {
            match credits.iter_mut().find(|(name, _)| name == &bill.vendor) {
                Some((_, held)) => *held = held.plus(&out.abs()),
                None => credits.push((bill.vendor.clone(), out.abs())),
            }
        }
    }

    let mut left: Vec<BillOwed> = Vec::new();
    for bill in open {
        let outstanding = bill.outstanding();
        if outstanding.is_zero() || outstanding.is_negative() {
            continue;
        }
        left.push(BillOwed {
            vendor: bill.vendor.clone(),
            display: bill.display(),
            number: bill.number.clone(),
            outstanding,
        });
    }
    Ok(OwedOnADay {
        bills: left,
        overpaid: credits,
    })
}

/// `GET /aged-payables`: what is owed, to whom, how late, and a sentence saying what to pay.
pub fn aged_payables<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let bills = Records::new(graph, VENDOR_BILL).list(scope, 5000)?;
    let payments = Records::new(graph, CASH_DISBURSEMENTS_JOURNAL).list(scope, 5000)?;

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
            let newest = bills
                .iter()
                .map(|row| field(row, "invoice_date").to_string())
                .chain(
                    payments
                        .iter()
                        .map(|row| field(row, "payment_date").to_string()),
                )
                .filter(|date| readable(date))
                .max();
            match newest {
                Some(day) => (day, "the newest date this Book holds"),
                None => {
                    return Err(StoreError::Invalid(
                        "this Book holds no dated bill or payment, so name the day to age to with `as_of_date`"
                            .into(),
                    ))
                }
            }
        }
    };

    // the same reading the fact engine uses, so a fact that pays a supplier and this report cannot disagree
    let OwedOnADay {
        bills: still_owed,
        overpaid,
    } = open_bills(graph, scope, &as_of)?;

    let mut undated = 0;
    let mut open: Vec<Owed> = Vec::new();
    for row in &bills {
        if is_void(field(row, "status")) {
            continue;
        }
        let raised = field(row, "invoice_date").to_string();
        match days_between(&as_of, &raised) {
            Some(signed) if signed < 0 => continue,
            Some(_) => {}
            None => {
                undated += 1;
                continue;
            }
        }
        let due = field(row, "due_date").trim().to_string();
        let due_readable = !due.is_empty() && readable(&due);
        open.push(Owed {
            vendor: field(row, "vendor_id").to_string(),
            name: field(row, "vendor_name").to_string(),
            number: field(row, "bill_number").to_string(),
            basis: if due_readable { due } else { raised.clone() },
            invoice_date: raised,
            from_own_date: !due_readable,
            amount: amount(&json!({"amount": field(row, "total_amount")}), "amount")?,
            paid: Money::zero(),
        });
    }
    open.sort_by(|left, right| {
        left.invoice_date
            .cmp(&right.invoice_date)
            .then_with(|| left.number.cmp(&right.number))
    });

    // what each bill still owes, from the one reading of the payments
    for bill in &mut open {
        let left = still_owed
            .iter()
            .find(|owed| owed.number.eq_ignore_ascii_case(&bill.number))
            .map(|owed| owed.outstanding.clone())
            .unwrap_or_else(Money::zero);
        bill.paid = bill.amount.minus(&left);
    }

    let mut vendors: Vec<&str> = open.iter().map(|bill| bill.vendor.as_str()).collect();
    vendors.sort_unstable();
    vendors.dedup();

    let mut rows: Vec<Value> = Vec::new();
    let mut owed_all = Money::zero();
    let mut late_all = Money::zero();
    let mut credit_all = Money::zero();
    let mut oldest_late: i64 = 0;
    let mut count = 0;

    for vendor in vendors {
        let mine: Vec<&Owed> = open.iter().filter(|bill| bill.vendor == vendor).collect();
        let mut owed = Money::zero();
        let mut late = Money::zero();
        let mut not_due = Money::zero();
        let mut bands: Vec<(&'static str, Money)> = ["1_30", "31_60", "61_90", "over_90"]
            .iter()
            .map(|band| (*band, Money::zero()))
            .collect();
        let mut items: Vec<Value> = Vec::new();
        let mut worst: Option<(i64, &Owed, Money)> = None;
        let mut soonest: Option<(i64, &Owed)> = None;

        for bill in &mine {
            let outstanding = bill.outstanding();
            if outstanding.is_negative() || outstanding.is_zero() {
                continue;
            }
            let days_late = days_between(&as_of, &bill.basis).unwrap_or(0);
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
                        worst = Some((days_late, bill, outstanding.clone()));
                    }
                }
                None => {
                    not_due = not_due.plus(&outstanding);
                    if soonest
                        .as_ref()
                        .map(|(days, _)| days_late < *days)
                        .unwrap_or(true)
                    {
                        soonest = Some((days_late, bill));
                    }
                }
            }
            items.push(json!({
                "bill_number": bill.number,
                "bill_date": bill.invoice_date,
                "due_date": if bill.from_own_date { Value::Null } else { json!(bill.basis) },
                "aged_from": if bill.from_own_date {
                    "its own date, because no due date was recorded"
                } else {
                    "its due date"
                },
                "bill_amount": wire(&bill.amount),
                "paid": wire(&bill.paid),
                "outstanding": wire(&outstanding),
                "days_late": if days_late > 0 { json!(days_late) } else { Value::Null },
                "band": band(days_late).unwrap_or("not_due"),
            }));
        }

        // money a supplier was paid past what was billed is theirs, so it is shown rather than netted against another
        // supplier's bill
        let credit = overpaid
            .iter()
            .find(|(name, _)| name == vendor)
            .map(|(_, held)| held.clone())
            .unwrap_or_else(Money::zero);
        if owed.is_zero() && credit.is_zero() {
            continue;
        }
        count += 1;
        owed_all = owed_all.plus(&owed);
        late_all = late_all.plus(&late);
        credit_all = credit_all.plus(&credit);

        let display = mine
            .first()
            .map(|row| row.display())
            .unwrap_or_else(|| vendor.to_string());
        if let Some((days, _, _)) = &worst {
            if *days > oldest_late {
                oldest_late = *days;
            }
        }
        let reminder = match (&worst, &soonest) {
            (Some((days, bill, amount)), _) => format!(
                "{display} is owed {owed}. The oldest unpaid bill, {} for {amount}, fell due on {} and is {days} days late.",
                bill.number,
                bill.basis
            ),
            (None, Some((_, bill))) if !owed.is_zero() => format!(
                "{display} is owed {owed} and nothing is late yet; the earliest falls due on {}.",
                bill.basis
            ),
            (None, None) if !credit.is_zero() => format!(
                "{display} has been paid {credit} more than was billed, so nothing is owed."
            ),
            _ => format!("{display} is owed {owed}."),
        };

        let mut row = json!({
            "vendor_id": vendor,
            "vendor_name": display,
            "owed": wire(&owed),
            "late": wire(&late),
            "not_due": wire(&not_due),
            "in_credit": wire(&credit),
            "bands": {
                "1_30": wire(&bands[0].1),
                "31_60": wire(&bands[1].1),
                "61_90": wire(&bands[2].1),
                "over_90": wire(&bands[3].1),
            },
            "bills": items,
            "reminder": reminder,
        });
        if credit.is_zero() {
            if let Value::Object(ref mut fields) = row {
                fields.remove("in_credit");
            }
        }
        rows.push(row);
    }

    let summary = if count == 0 {
        format!("Nothing is owed to anyone on {as_of}.")
    } else if late_all.is_zero() {
        format!(
            "{} owed across {count} {} on {as_of}, none of it late yet.",
            wire(&owed_all),
            if count == 1 { "supplier" } else { "suppliers" }
        )
    } else {
        format!(
            "{} owed across {count} {}, of which {} is late and the oldest by {oldest_late} days.",
            wire(&owed_all),
            if count == 1 { "supplier" } else { "suppliers" },
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
            "vendors": count,
            "oldest_late_days": oldest_late,
        },
        "vendors": rows,
        "not_aged": undated,
        "summary": summary,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn a_bill(
        graph: &MemoryGraph,
        number: &str,
        vendor: &str,
        raised: &str,
        due: &str,
        total: &str,
    ) {
        Records::new(graph, VENDOR_BILL)
            .create(
                &scope(),
                &json!({"bill_number": number, "vendor_id": vendor,
                        "vendor_name": format!("{vendor} Traders"), "invoice_date": raised,
                        "due_date": due, "total_amount": total, "status": "pending"}),
            )
            .expect("a bill");
    }

    fn a_payment(
        graph: &MemoryGraph,
        number: &str,
        vendor: &str,
        paid: &str,
        amount: &str,
        document: &str,
    ) {
        Records::new(graph, CASH_DISBURSEMENTS_JOURNAL)
            .create(
                &scope(),
                &json!({"payment_number": number, "vendor_id": vendor, "payment_date": paid,
                        "amount": amount, "document_reference": document, "status": "posted"}),
            )
            .expect("a payment");
    }

    fn aged(graph: &MemoryGraph, day: &str) -> Value {
        aged_payables(graph, &scope(), &json!({"as_of_date": day})).expect("what is owed")
    }

    #[test]
    fn a_bill_that_is_late_is_counted_in_the_band_it_is_late_in() {
        let graph = MemoryGraph::new();
        a_bill(&graph, "B-1", "steel", "2026-09-01", "2026-09-30", "500.00");
        let answer = aged(&graph, "2026-11-25");
        assert_eq!(answer["totals"]["owed"], json!("500.000000"));
        // 30 September to 25 November is 56 days, so the whole bill sits in the 31 to 60 band
        assert_eq!(answer["vendors"][0]["bands"]["31_60"], json!("500.000000"));
        assert_eq!(answer["vendors"][0]["bands"]["1_30"], json!("0.000000"));
        assert_eq!(answer["vendors"][0]["bills"][0]["days_late"], json!(56));
        let sentence = answer["vendors"][0]["reminder"]
            .as_str()
            .expect("a sentence");
        // a person is told who, how much, which bill and how late, in one sentence they can act on
        assert!(
            sentence.contains("steel Traders is owed 500.00"),
            "{sentence}"
        );
        assert!(sentence.contains("B-1"), "{sentence}");
        assert!(sentence.contains("56 days late"), "{sentence}");
    }

    #[test]
    fn a_payment_that_names_a_bill_pays_that_bill_and_leaves_the_rest() {
        let graph = MemoryGraph::new();
        a_bill(&graph, "B-1", "steel", "2026-09-01", "2026-09-30", "500.00");
        a_bill(&graph, "B-2", "steel", "2026-10-01", "2026-10-31", "300.00");
        a_payment(&graph, "P-1", "steel", "2026-11-02", "200.00", "B-2");
        let answer = aged(&graph, "2026-11-25");
        assert_eq!(answer["totals"]["owed"], json!("600.000000"));
        let bills = answer["vendors"][0]["bills"].as_array().expect("bills");
        assert_eq!(bills[0]["bill_number"], json!("B-1"));
        assert_eq!(bills[0]["outstanding"], json!("500.000000"));
        assert_eq!(bills[1]["bill_number"], json!("B-2"));
        assert_eq!(bills[1]["outstanding"], json!("100.000000"));
    }

    #[test]
    fn a_payment_with_no_note_pays_the_oldest_bill_first_as_a_person_would() {
        let graph = MemoryGraph::new();
        a_bill(&graph, "B-1", "steel", "2026-09-01", "2026-09-30", "500.00");
        a_bill(&graph, "B-2", "steel", "2026-10-01", "2026-10-31", "300.00");
        a_payment(&graph, "P-1", "steel", "2026-11-02", "500.00", "");
        let answer = aged(&graph, "2026-11-25");
        // the older bill is cleared and the newer one keeps what the money did not reach
        assert_eq!(answer["totals"]["owed"], json!("300.000000"));
        let bills = answer["vendors"][0]["bills"].as_array().expect("bills");
        assert_eq!(bills.len(), 1);
        assert_eq!(bills[0]["bill_number"], json!("B-2"));
    }

    #[test]
    fn a_supplier_paid_more_than_billed_is_shown_in_credit_rather_than_netted_away() {
        let graph = MemoryGraph::new();
        a_bill(&graph, "B-1", "steel", "2026-09-01", "2026-09-30", "500.00");
        a_payment(&graph, "P-1", "steel", "2026-11-02", "600.00", "B-1");
        let answer = aged(&graph, "2026-11-25");
        assert_eq!(answer["totals"]["owed"], json!("0.000000"));
        assert_eq!(answer["totals"]["in_credit"], json!("100.000000"));
    }

    #[test]
    fn a_bill_with_no_due_date_is_aged_from_its_own_date_and_says_so() {
        let graph = MemoryGraph::new();
        a_bill(&graph, "B-1", "steel", "2026-11-01", "", "500.00");
        let answer = aged(&graph, "2026-11-25");
        assert_eq!(
            answer["vendors"][0]["bills"][0]["aged_from"],
            json!("its own date, because no due date was recorded")
        );
        assert_eq!(answer["vendors"][0]["bills"][0]["days_late"], json!(24));
    }

    #[test]
    fn a_bill_raised_after_the_day_being_aged_to_is_not_owed_on_that_day() {
        let graph = MemoryGraph::new();
        a_bill(&graph, "B-1", "steel", "2026-12-01", "2026-12-31", "500.00");
        let answer = aged(&graph, "2026-11-25");
        assert_eq!(answer["totals"]["vendors"], json!(0));
        assert_eq!(
            answer["summary"],
            json!("Nothing is owed to anyone on 2026-11-25.")
        );
    }

    #[test]
    fn a_void_bill_is_ignored_and_an_unreadable_date_is_counted_rather_than_dropped() {
        let graph = MemoryGraph::new();
        Records::new(&graph, VENDOR_BILL)
            .create(
                &scope(),
                &json!({"bill_number": "B-0", "vendor_id": "steel", "invoice_date": "2026-09-01",
                        "total_amount": "500.00", "status": "void"}),
            )
            .expect("a voided bill");
        a_bill(&graph, "B-1", "steel", "not a day", "2026-09-30", "500.00");
        let answer = aged(&graph, "2026-11-25");
        assert_eq!(answer["totals"]["vendors"], json!(0));
        assert_eq!(answer["not_aged"], json!(1));
    }

    #[test]
    fn a_day_that_is_not_a_day_is_refused_in_words() {
        let graph = MemoryGraph::new();
        let refused = aged_payables(&graph, &scope(), &json!({"as_of_date": "last Tuesday"}));
        assert!(refused.is_err());
    }

    #[test]
    fn the_reader_the_facts_use_agrees_with_what_the_report_says_is_owed() {
        let graph = MemoryGraph::new();
        a_bill(&graph, "B-1", "steel", "2026-09-01", "2026-09-30", "500.00");
        a_bill(&graph, "B-2", "steel", "2026-10-01", "2026-10-31", "300.00");
        a_payment(&graph, "P-1", "steel", "2026-11-02", "200.00", "B-2");
        let OwedOnADay {
            bills: open,
            overpaid,
        } = open_bills(&graph, &scope(), "2026-11-25").expect("what is owed");
        // oldest first, with the money that named B-2 already taken off it
        assert_eq!(open.len(), 2);
        assert_eq!(open[0].number, "B-1");
        assert_eq!(wire(&open[0].outstanding), "500.000000");
        assert_eq!(open[1].number, "B-2");
        assert_eq!(wire(&open[1].outstanding), "100.000000");
        assert_eq!(open[0].display, "steel Traders");
        assert!(overpaid.is_empty());
    }
}
