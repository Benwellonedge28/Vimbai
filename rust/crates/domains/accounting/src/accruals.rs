//! The period-end journals: a cost that belongs to a month nobody has billed yet, and money paid now for something that
//! will be used later.
//!
//! Both exist because a month's profit has to be true even when the paperwork is not. Two ordinary cases:
//!
//! * *An accrual.* The electricity for November is used in November and the bill arrives in December. If November's profit
//!   waited for the bill, it would look better than the month really was and December would carry a cost it did not cause.
//!   So November takes the cost on an estimate and owes it: the cost is charged and a liability stands in its place. When
//!   the bill does arrive it is posted as any bill is, and the estimate is reversed, so the month ends up carrying the real
//!   figure and not both;
//! * *A prepayment.* A year of insurance is paid in one go in January. The money has left, but eleven twelfths of it buys
//!   something the business has not used yet, so it is not January's cost: it is an asset that is spent down month by
//!   month. Each month takes its share out of the asset and into the cost, and what is left is what is still to be used.
//!
//! What this keeps, and why:
//!
//! * an accrual is an estimate, so it carries no tax. Tax belongs to the document that charges it, and an estimate is not
//!   one: when the bill arrives its tax is reclaimed there, once;
//! * a correction is a reversing entry, never an edit. The books are a record of what was believed at the time, so the
//!   estimate stays visible with its reversal beside it and the bill stands on its own;
//! * a prepayment's months are split exactly, with the odd cent going to the earliest months so the parts add back to what
//!   was paid. A year of 1,000.00 is not twelve times 83.33;
//! * a month that has been closed is not written into, and a month cannot be released twice: releasing it again would spend
//!   the same money a second time;
//! * money that left the account is recorded in the cash book in the same act as its double entry, so the cash book cannot
//!   believe the bank still holds money the ledger has already spent. Vimbai does not move the money: it records what
//!   already left.
//!
//! The accounts are found by what the Book's own chart calls them and by their type, never by a number, and a caller may
//! name them when a Book numbers its chart its own way.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, money, or_default, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, ACCRUAL, PREPAYMENT};

use crate::cashbook;
use crate::ledger_book;
use crate::periods;
use crate::subsidiary_journals::{account_named, must_be_open, positive, post};

/// The month a document belongs to, as a period, and the day it is dated in the books.
fn month_of(payload: &Value, key: &str) -> Result<(String, String), StoreError> {
    let stated = text(payload, key);
    let Some(period) = periods::period_of(&stated) else {
        return Err(refused(&format!(
            "Say which month this belongs to, as a period such as 2026-11, in {key}."
        )));
    };
    let Some(last) = periods::last_day_of(&period) else {
        return Err(refused("That period does not read as a month."));
    };
    Ok((period, last))
}

// ---------------------------------------------------------------------------------------------------------------
// Accruals: a cost that belongs to a month nobody has billed yet.
// ---------------------------------------------------------------------------------------------------------------

/// `POST /accruals`: charge a month with a cost it has used but not been billed for, and owe it.
pub fn record_accrual(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let (converted, document) = crate::document_currency::in_book_money(graph, scope, payload)?;
    let payload = &converted;
    let _ = &document;

    let amount = positive(payload, "amount", "An accrual")?;
    let (period, entry_date) = month_of(payload, "period")?;
    must_be_open(graph, scope, &entry_date)?;
    let expense = account_named(
        graph,
        scope,
        payload,
        "expense_account",
        "expense",
        "expense",
    )?;
    let accrued = account_named(
        graph,
        scope,
        payload,
        "accrued_account",
        "accrued",
        "liability",
    )?;
    if expense == accrued {
        return Err(refused(
            "The cost and the liability it stands in place of cannot be the same account.",
        ));
    }
    let what = text(payload, "description");
    let note = if what.trim().is_empty() {
        format!("Accrued cost for {period}")
    } else {
        format!("Accrued cost for {period}: {what}")
    };
    // one estimate per cost per month, named so: the ledger refuses a reference it has already posted, which is what stops
    // the same month being charged twice by a second press of the button
    let reference = format!("ACR-{period}-{expense}");
    // dated in the month whose profit it changes, not on the day somebody pressed the button
    let dated = json!({"entry_date": entry_date, "created_at": text(payload, "created_at")});
    let entry_id = post(
        graph,
        scope,
        &dated,
        format!("Accruals Journal: {reference}"),
        &reference,
        "AccrualsJournal",
        &expense,
        &accrued,
        &amount,
        &note,
    )?;
    remember(
        graph,
        scope,
        ACCRUAL,
        json!({
            "period": period,
            "entry_date": entry_date,
            "expense_account": expense,
            "accrued_account": accrued,
            "amount": wire(&amount),
            "description": what,
            "document_currency": text(payload, "document_currency"),
            "document_rate": text(payload, "document_rate"),
            "document_amount": text(payload, "document_amount"),
            "journal_entry_id": entry_id,
            "reversal_entry_id": "",
            "reversed_on": "",
            "notes": text(payload, "notes"),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `POST /accruals/reverse`: take an estimate back out, by reversing entry.
///
/// The estimate is not edited and it is not deleted: the month it belongs to stays as it was believed, and the reversal is
/// what undoes it. Reversing it again is refused, because one estimate has one reversal.
pub fn reverse_accrual(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let accrual_id = text(payload, "accrual_id");
    if accrual_id.trim().is_empty() {
        return Err(refused("Say which accrual to reverse."));
    }
    let held = Records::new(graph, ACCRUAL);
    let accrual = held.get(scope, accrual_id.trim())?;
    if !field(&accrual, "reversal_entry_id").trim().is_empty() {
        return Err(refused(&format!(
            "The accrual for {} has already been reversed. One estimate has one reversal.",
            field(&accrual, "period")
        )));
    }
    // a reversal dated today would land in whatever month the machine thinks it is, so it is dated either where the
    // caller says or at the start of the month after the one the estimate belongs to
    let stated = text(payload, "reversed_on");
    let on = if stated.trim().is_empty() {
        periods::first_day_after(field(&accrual, "period"))
    } else {
        stated.trim().to_string()
    };
    must_be_open(graph, scope, &on)?;
    let reversal =
        ledger_book::reverse_entry(graph, scope, field(&accrual, "journal_entry_id"), Some(&on))?;
    let reversal_id = field(&reversal, "id").to_string();
    let updated = held.update(
        scope,
        accrual_id.trim(),
        &json!({"reversal_entry_id": reversal_id, "reversed_on": on}),
    )?;
    Ok(updated)
}

/// `GET /accruals`: what the Book has accrued, newest month first, with the ones still standing marked.
pub fn list_accruals<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let period = text(query, "period");
    let mut rows: Vec<Value> = Records::new(graph, ACCRUAL)
        .list(scope, 5000)?
        .into_iter()
        .filter(|row| period.trim().is_empty() || field(row, "period") == period.trim())
        .collect();
    rows.sort_by(|a, b| {
        field(b, "period")
            .cmp(field(a, "period"))
            .then_with(|| field(a, "expense_account").cmp(field(b, "expense_account")))
    });
    Ok(Value::Array(rows))
}

// ---------------------------------------------------------------------------------------------------------------
// Prepayments: money paid now for something the business has not used yet.
// ---------------------------------------------------------------------------------------------------------------

/// The months a cover runs for, from the month it starts, as `2026-01` and on.
fn cover_months(from: &str, months: i64) -> Result<Vec<String>, StoreError> {
    let Some(start) = periods::period_of(from) else {
        return Err(refused(
            "Say which month the cover starts, as a period such as 2026-01, in covers_from.",
        ));
    };
    let year: i32 = start[0..4]
        .parse()
        .map_err(|_| refused("That period does not read as a month."))?;
    let month: i32 = start[5..7]
        .parse()
        .map_err(|_| refused("That period does not read as a month."))?;
    let mut covered = Vec::new();
    for step in 0..months {
        let walked = month + step as i32 - 1;
        let year = year + walked / 12;
        let month = walked % 12 + 1;
        covered.push(format!("{year:04}-{month:02}"));
    }
    Ok(covered)
}

/// What each month of a cover costs, split exactly: the odd cent goes to the earliest months so the parts add back to what
/// was paid rather than leaving a fraction nobody can spend.
fn share_out(total: &Money, months: usize) -> Vec<Money> {
    let weights = vec![Money::from_i64(1); months];
    total.allocate(&weights, 2)
}

/// `POST /prepayments`: money that has already left for something the business will use over several months.
pub fn record_prepayment(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let (converted, document) = crate::document_currency::in_book_money(graph, scope, payload)?;
    let payload = &converted;
    let _ = &document;

    let reference = text(payload, "reference_number");
    if reference.trim().is_empty() {
        return Err(refused(
            "Give the prepayment a reference of your own, so the same one cannot be recorded twice.",
        ));
    }
    let total = positive(payload, "total_amount", "A prepayment")?;
    let months = text(payload, "months").trim().parse::<i64>().unwrap_or(0);
    if months < 1 {
        return Err(refused(
            "Say how many months the cover runs for, so each month can take its share.",
        ));
    }
    let held_in = text(payload, "paid_from");
    if held_in.trim().is_empty() {
        return Err(refused(
            "Say which cash or bank account the money left, so the payment is recorded where the money is kept.",
        ));
    }
    let money_account = text(payload, "money_account");
    if money_account.trim().is_empty() {
        return Err(refused(
            "Say which account in the chart the money sits in, so the payment is posted against it as well as recorded in the cash book.",
        ));
    }
    let prepaid = account_named(graph, scope, payload, "prepaid_account", "prepaid", "asset")?;
    let expense = account_named(
        graph,
        scope,
        payload,
        "expense_account",
        "expense",
        "expense",
    )?;
    if prepaid == expense {
        return Err(refused(
            "What is still to be used and what has been used cannot be the same account.",
        ));
    }
    let covered = cover_months(&text(payload, "covers_from"), months)?;
    let shares = share_out(&total, covered.len());
    let schedule: Vec<Value> = covered
        .iter()
        .zip(shares.iter())
        .map(|(period, share)| json!({"period": period, "amount": wire(share)}))
        .collect();
    // the first month is the one the cover starts in, which is the day the money was paid for it
    let entry_date = periods::last_day_of(&covered[0])
        .ok_or_else(|| refused("That period does not read as a month."))?;
    must_be_open(graph, scope, &entry_date)?;
    let what = text(payload, "description");
    let note = if what.trim().is_empty() {
        format!("Prepaid {reference}")
    } else {
        format!("Prepaid {reference}: {what}")
    };
    let dated = json!({"entry_date": entry_date, "created_at": text(payload, "created_at")});
    let entry_id = post(
        graph,
        scope,
        &dated,
        format!("Prepayments Journal: {reference}"),
        &reference,
        "PrepaymentsJournal",
        &prepaid,
        &money_account,
        &total,
        &note,
    )?;
    let stated_total = text(payload, "document_total");
    let cash_amount = if stated_total.trim().is_empty() {
        total.to_fixed(6)
    } else {
        stated_total
    };
    // money that already left is recorded where the money the business holds is kept, in the same act as its double entry
    let cash = cashbook::create_entry(
        graph,
        scope,
        &json!({
            "bank_account": held_in.trim(),
            "account_code": prepaid,
            "entry_date": entry_date,
            "voucher_number": reference,
            "description": note,
            // the figure that left the account, in the money it left in, exactly as it was paid
            "amount": cash_amount,
            "currency": or_default(payload, "document_currency", "USD"),
            "exchange_rate": or_default(payload, "document_rate", "1"),
            "is_debit": false,
            "book_type": "payments",
            "status": "posted",
        }),
    )?;
    remember(
        graph,
        scope,
        PREPAYMENT,
        json!({
            "reference_number": reference,
            "description": what,
            "total_amount": wire(&total),
            "months": months,
            "covers_from": text(payload, "covers_from"),
            "prepaid_account": prepaid,
            "expense_account": expense,
            "paid_from": held_in.trim(),
            "money_account": money_account.trim(),
            "schedule": schedule,
            "released": Vec::<String>::new(),
            "document_currency": text(payload, "document_currency"),
            "document_rate": text(payload, "document_rate"),
            "document_total": text(payload, "document_total"),
            "journal_entry_id": entry_id,
            "cash_entry_id": field(&cash, "id"),
            "notes": text(payload, "notes"),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// What a prepayment's month is, and what is left of it after the months already released.
fn still_to_use(prepayment: &Value) -> Result<Money, StoreError> {
    let total = money(prepayment, "total_amount")?;
    let mut used = Money::zero();
    for period in prepayment
        .get("released")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let Some(period) = period.as_str() else {
            continue;
        };
        if let Some(share) = prepayment
            .get("schedule")
            .and_then(Value::as_array)
            .and_then(|schedule| {
                schedule
                    .iter()
                    .find(|row| field(row, "period") == period)
                    .cloned()
            })
        {
            used = used.plus(&vimbai_store::amounts::stored_amount(&share, "amount")?);
        }
    }
    Ok(total.minus(&used))
}

/// `POST /prepayments/release`: one month's share of a prepayment leaves the asset and becomes a cost.
pub fn release_prepayment(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let prepayment_id = text(payload, "prepayment_id");
    if prepayment_id.trim().is_empty() {
        return Err(refused("Say which prepayment to release a month of."));
    }
    let stated = text(payload, "period");
    let Some(period) = periods::period_of(&stated) else {
        return Err(refused(
            "Say which month to release, as a period such as 2026-11.",
        ));
    };
    let held = Records::new(graph, PREPAYMENT);
    let prepayment = held.get(scope, prepayment_id.trim())?;
    let released: Vec<String> = prepayment
        .get("released")
        .and_then(Value::as_array)
        .map(|periods| {
            periods
                .iter()
                .filter_map(|period| period.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if released.iter().any(|already| already == &period) {
        return Err(refused(&format!(
            "{period} has already been released for this prepayment. Releasing it again would spend the same money twice."
        )));
    }
    let share = prepayment
        .get("schedule")
        .and_then(Value::as_array)
        .and_then(|schedule| {
            schedule
                .iter()
                .find(|row| field(row, "period") == period)
                .cloned()
        })
        .ok_or_else(|| {
            refused(&format!(
                "{period} is not one of the months this prepayment covers, so there is nothing of it to release in that month."
            ))
        })?;
    let amount = vimbai_store::amounts::stored_amount(&share, "amount")?;
    let entry_date = periods::last_day_of(&period)
        .ok_or_else(|| refused("That period does not read as a month."))?;
    must_be_open(graph, scope, &entry_date)?;
    let reference = format!("PRE-{}-{period}", field(&prepayment, "reference_number"));
    let dated = json!({"entry_date": entry_date, "created_at": text(payload, "created_at")});
    let entry_id = post(
        graph,
        scope,
        &dated,
        format!("Prepayments Journal: {reference}"),
        &reference,
        "PrepaymentsJournal",
        field(&prepayment, "expense_account"),
        field(&prepayment, "prepaid_account"),
        &amount,
        &format!(
            "{period} share of prepayment {}",
            field(&prepayment, "reference_number")
        ),
    )?;
    let mut now_released = released;
    now_released.push(period.clone());
    let updated = held.update(
        scope,
        prepayment_id.trim(),
        &json!({"released": now_released, "released_entry_id": entry_id}),
    )?;
    // what is left of the prepayment after this month, so the caller does not have to work it out
    let left = still_to_use(&updated)?.to_fixed(2);
    let mut answer = updated;
    if let Some(object) = answer.as_object_mut() {
        object.insert("still_to_use".to_string(), json!(left));
    }
    Ok(answer)
}

/// `GET /prepayments`: what has been paid in advance, with what is still to be used on each.
pub fn list_prepayments<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let account = text(query, "expense_account");
    let rows: Vec<Value> = Records::new(graph, PREPAYMENT)
        .list(scope, 5000)?
        .into_iter()
        .filter(|row| account.trim().is_empty() || field(row, "expense_account") == account.trim())
        .collect();
    let mut stated = Vec::new();
    for mut row in rows {
        let left = still_to_use(&row)?.to_fixed(2);
        if let Some(object) = row.as_object_mut() {
            object.insert("still_to_use".to_string(), json!(left));
        }
        stated.push(row);
    }
    Ok(Value::Array(stated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn business() -> (MemoryGraph, BookScope) {
        let graph = MemoryGraph::new();
        let scope = scope();
        crate::chart::seed_opening_chart(&graph, &scope, "business").expect("a chart");
        (graph, scope)
    }

    fn lines(graph: &MemoryGraph, scope: &BookScope, entry_id: &str) -> Vec<Value> {
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

    fn debit(lines: &[Value], account: &str) -> Money {
        money(&line(lines, account), "debit").expect("a debit")
    }

    fn credit(lines: &[Value], account: &str) -> Money {
        money(&line(lines, account), "credit").expect("a credit")
    }

    fn amount(text: &str) -> Money {
        Money::parse(text).expect("an amount")
    }

    fn cash_account(graph: &MemoryGraph, scope: &BookScope) -> String {
        let cash = cashbook::create_account(
            graph,
            scope,
            &json!({"account_name": "Bank", "account_type": "bank",
                    "opening_balance": "5000.00", "currency": "USD"}),
        )
        .expect("a bank account");
        field(&cash, "id").to_string()
    }

    // ---- accruals ----

    /// The month carries the cost and owes it: nothing has been billed, so nothing is paid, and the cost is an expense
    /// with a liability standing in its place.
    #[test]
    fn an_accrual_charges_the_month_and_owes_the_cost() {
        let (graph, scope) = business();
        let accrual = record_accrual(
            &graph,
            &scope,
            &json!({"period": "2026-11", "amount": "480.00",
                    "expense_account": "6100", "description": "Electricity"}),
        )
        .expect("accrued");

        assert_eq!(field(&accrual, "period"), "2026-11");
        let entry_id = field(&accrual, "journal_entry_id").to_string();
        let posted = lines(&graph, &scope, &entry_id);
        assert_eq!(debit(&posted, "6100"), amount("480.00"));
        assert_eq!(credit(&posted, "2200"), amount("480.00"));
        // dated in the month it belongs to, because that is the month whose profit it changes
        let entry = crate::ledger_book::get_entry(&graph, &scope, &entry_id).expect("entry");
        assert_eq!(field(&entry, "entry_date"), "2026-11-30");
    }

    /// The estimate is taken back by a reversing entry, not by an edit: the month stays as it was believed, the reversal
    /// undoes it, and the two together leave nothing behind.
    #[test]
    fn reversing_an_accrual_undoes_it_without_editing_it() {
        let (graph, scope) = business();
        let accrual = record_accrual(
            &graph,
            &scope,
            &json!({"period": "2026-11", "amount": "480.00", "expense_account": "6100"}),
        )
        .expect("accrued");
        let reversed = reverse_accrual(
            &graph,
            &scope,
            &json!({"accrual_id": field(&accrual, "id")}),
        )
        .expect("reversed");

        // the reversal is dated at the start of the next month, so November keeps its estimate and December is clean
        assert_eq!(field(&reversed, "reversed_on"), "2026-12-01");
        let reversal_id = field(&reversed, "reversal_entry_id").to_string();
        let posted = lines(&graph, &scope, &reversal_id);
        assert_eq!(credit(&posted, "6100"), amount("480.00"));
        assert_eq!(debit(&posted, "2200"), amount("480.00"));
        let entry = crate::ledger_book::get_entry(&graph, &scope, &reversal_id).expect("entry");
        assert_eq!(
            field(&entry, "reverses_entry_id"),
            field(&accrual, "journal_entry_id")
        );
    }

    #[test]
    fn the_same_estimate_cannot_be_taken_back_twice() {
        let (graph, scope) = business();
        let accrual = record_accrual(
            &graph,
            &scope,
            &json!({"period": "2026-11", "amount": "480.00", "expense_account": "6100"}),
        )
        .expect("accrued");
        reverse_accrual(
            &graph,
            &scope,
            &json!({"accrual_id": field(&accrual, "id")}),
        )
        .expect("reversed");
        let again = reverse_accrual(
            &graph,
            &scope,
            &json!({"accrual_id": field(&accrual, "id")}),
        )
        .expect_err("already reversed");
        assert!(format!("{again}").contains("already been reversed"));
    }

    /// A month is charged once for one cost: the reference says which month and which cost, so a second press is refused
    /// by the ledger rather than doubling the estimate.
    #[test]
    fn the_same_month_of_the_same_cost_cannot_be_accrued_twice() {
        let (graph, scope) = business();
        let payload = json!({"period": "2026-11", "amount": "480.00", "expense_account": "6100"});
        record_accrual(&graph, &scope, &payload).expect("accrued");
        let again = record_accrual(&graph, &scope, &payload).expect_err("a second estimate");
        assert!(format!("{again}").contains("already exists"));
    }

    #[test]
    fn an_accrual_of_nothing_and_a_missing_month_are_refused() {
        let (graph, scope) = business();
        let nothing = record_accrual(
            &graph,
            &scope,
            &json!({"period": "2026-11", "amount": "0", "expense_account": "6100"}),
        )
        .expect_err("nothing");
        assert!(format!("{nothing}").contains("more than nothing"));

        let no_month = record_accrual(
            &graph,
            &scope,
            &json!({"period": "November", "amount": "10.00", "expense_account": "6100"}),
        )
        .expect_err("not a month");
        assert!(format!("{no_month}").contains("as a period such as"));
    }

    #[test]
    fn a_closed_month_is_refused_for_an_accrual() {
        let (graph, scope) = business();
        periods::close_period(
            &graph,
            &scope,
            &json!({"period": "2026-11", "closes_on": "2026-11-30"}),
        )
        .expect("closed");
        let refused = record_accrual(
            &graph,
            &scope,
            &json!({"period": "2026-11", "amount": "10.00", "expense_account": "6100"}),
        )
        .expect_err("closed month");
        assert!(format!("{refused}").contains("2026-11"));
    }

    #[test]
    fn a_book_without_an_accrual_account_says_which_one_is_missing() {
        let graph = MemoryGraph::new();
        let scope = scope();
        crate::chart::seed_opening_chart(&graph, &scope, "personal").expect("a chart");
        let refused = record_accrual(
            &graph,
            &scope,
            &json!({"period": "2026-11", "amount": "10.00", "expense_account": "6400"}),
        )
        .expect_err("no accrued account");
        assert!(format!("{refused}").contains("no account set aside for accrued"));
    }

    // ---- prepayments ----

    fn a_year_of_insurance(graph: &MemoryGraph, scope: &BookScope, cash: &str) -> Value {
        record_prepayment(
            graph,
            scope,
            &json!({"reference_number": "INS-1", "total_amount": "1200.00", "months": "12",
                    "covers_from": "2026-01", "paid_from": cash, "money_account": "1010",
                    "expense_account": "6300", "description": "Insurance"}),
        )
        .expect("prepaid")
    }

    /// Money paid for a year is an asset, not January's cost: it leaves the bank and sits as something not yet used.
    #[test]
    fn a_year_paid_up_front_is_an_asset_and_the_money_is_recorded_leaving() {
        let (graph, scope) = business();
        let cash = cash_account(&graph, &scope);
        let prepayment = a_year_of_insurance(&graph, &scope, &cash);

        let entry_id = field(&prepayment, "journal_entry_id").to_string();
        let posted = lines(&graph, &scope, &entry_id);
        assert_eq!(debit(&posted, "1300"), amount("1200.00"));
        assert_eq!(credit(&posted, "1010"), amount("1200.00"));
        // the money already left, so the cash book says so in the same act
        assert!(!field(&prepayment, "cash_entry_id").is_empty());
        assert_eq!(field(&prepayment, "still_to_use"), "");
    }

    /// Each month takes its share, and the shares add back to what was paid exactly: a year of 1,000.00 is not twelve
    /// times 83.33.
    #[test]
    fn the_months_are_split_exactly_and_add_back_to_what_was_paid() {
        let (graph, scope) = business();
        let cash = cash_account(&graph, &scope);
        let prepayment = record_prepayment(
            &graph,
            &scope,
            &json!({"reference_number": "INS-2", "total_amount": "1000.00", "months": "12",
                    "covers_from": "2026-01", "paid_from": cash, "money_account": "1010",
                    "expense_account": "6300"}),
        )
        .expect("prepaid");

        let schedule = prepayment["schedule"].as_array().expect("a schedule");
        assert_eq!(schedule.len(), 12);
        let mut total = Money::zero();
        for row in schedule {
            total =
                total.plus(&vimbai_store::amounts::stored_amount(row, "amount").expect("a share"));
        }
        assert_eq!(total, amount("1000.00"));
        // the odd cent goes to the earliest months rather than leaving a fraction nobody can spend
        assert_eq!(
            vimbai_store::amounts::stored_amount(&schedule[0], "amount").expect("a share"),
            amount("83.34")
        );
        assert_eq!(
            vimbai_store::amounts::stored_amount(&schedule[11], "amount").expect("a share"),
            amount("83.33")
        );
    }

    /// Releasing a month turns that month's share from something not yet used into a cost, and what is left of the
    /// prepayment goes down by exactly that much.
    #[test]
    fn releasing_a_month_makes_its_share_a_cost() {
        let (graph, scope) = business();
        let cash = cash_account(&graph, &scope);
        let prepayment = a_year_of_insurance(&graph, &scope, &cash);

        let released = release_prepayment(
            &graph,
            &scope,
            &json!({"prepayment_id": field(&prepayment, "id"), "period": "2026-02"}),
        )
        .expect("released");

        assert_eq!(field(&released, "still_to_use"), "1100.00");
        let entry_id = field(&released, "released_entry_id").to_string();
        let posted = lines(&graph, &scope, &entry_id);
        assert_eq!(debit(&posted, "6300"), amount("100.00"));
        assert_eq!(credit(&posted, "1300"), amount("100.00"));
        let entry = crate::ledger_book::get_entry(&graph, &scope, &entry_id).expect("entry");
        assert_eq!(field(&entry, "entry_date"), "2026-02-28");
    }

    #[test]
    fn the_same_month_cannot_be_released_twice() {
        let (graph, scope) = business();
        let cash = cash_account(&graph, &scope);
        let prepayment = a_year_of_insurance(&graph, &scope, &cash);
        release_prepayment(
            &graph,
            &scope,
            &json!({"prepayment_id": field(&prepayment, "id"), "period": "2026-01"}),
        )
        .expect("released");
        let again = release_prepayment(
            &graph,
            &scope,
            &json!({"prepayment_id": field(&prepayment, "id"), "period": "2026-01"}),
        )
        .expect_err("already released");
        assert!(format!("{again}").contains("already been released"));
    }

    /// A month the cover does not run to has nothing of it to release, and the answer says so rather than spending the
    /// asset in a month it does not belong to.
    #[test]
    fn a_month_outside_the_cover_has_nothing_to_release() {
        let (graph, scope) = business();
        let cash = cash_account(&graph, &scope);
        let prepayment = a_year_of_insurance(&graph, &scope, &cash);
        let refused = release_prepayment(
            &graph,
            &scope,
            &json!({"prepayment_id": field(&prepayment, "id"), "period": "2027-03"}),
        )
        .expect_err("outside the cover");
        assert!(format!("{refused}").contains("not one of the months"));
    }

    #[test]
    fn a_prepayment_without_months_or_a_reference_is_refused() {
        let (graph, scope) = business();
        let cash = cash_account(&graph, &scope);
        let no_months = record_prepayment(
            &graph,
            &scope,
            &json!({"reference_number": "INS-3", "total_amount": "1200.00", "covers_from": "2026-01",
                    "paid_from": cash, "money_account": "1010", "expense_account": "6300"}),
        )
        .expect_err("no months");
        assert!(format!("{no_months}").contains("how many months"));

        let no_reference = record_prepayment(
            &graph,
            &scope,
            &json!({"total_amount": "1200.00", "months": "12", "covers_from": "2026-01",
                    "paid_from": cash, "money_account": "1010", "expense_account": "6300"}),
        )
        .expect_err("no reference");
        assert!(format!("{no_reference}").contains("reference of your own"));
    }

    /// What is still to be used is stated on each prepayment, so a Book can see what it has paid for and not yet had.
    #[test]
    fn what_is_still_to_be_used_is_stated_on_each_prepayment() {
        let (graph, scope) = business();
        let cash = cash_account(&graph, &scope);
        let prepayment = a_year_of_insurance(&graph, &scope, &cash);
        release_prepayment(
            &graph,
            &scope,
            &json!({"prepayment_id": field(&prepayment, "id"), "period": "2026-01"}),
        )
        .expect("released");

        let listed = list_prepayments(&graph, &scope, &json!({})).expect("listed");
        assert_eq!(listed.as_array().expect("a list").len(), 1);
        assert_eq!(field(&listed[0], "still_to_use"), "1100.00");

        let accruals = list_accruals(&graph, &scope, &json!({})).expect("listed");
        assert!(accruals.as_array().expect("a list").is_empty());
    }
}
