#![forbid(unsafe_code)]
//! Closed periods: the months a Book has reported on, and will not be written into again.
//!
//! A set of books is worth nothing the moment last month can change after somebody has read it. Reports are produced, a
//! bank statement is reconciled, a tax return is filed, and then a late invoice is typed with a date three weeks back and
//! every figure that was reported quietly becomes a different figure. Nothing looks broken; the numbers simply stop
//! matching what was sent.
//!
//! So a month can be closed. Closing is a decision recorded against the Book, not an edit to the journal: the entries
//! that are there stay exactly as they are, and what changes is that the posting check refuses anything dated inside a
//! closed month. A correction to a closed month is therefore an entry in the current month, which is what an accountant
//! does anyway, and reopening is a separate decision with its own record.
//!
//! Two rules are refused here rather than stored: a period that does not read as a month, and closing the same month
//! twice. A period later than the day of the close is refused too, because closing a month nobody has traded through yet
//! would block the very entries that belong in it.

use serde_json::{json, Value};
use vimbai_store::read::{field, refused, text};
use vimbai_store::{BookScope, Graph, Records, StoreError, CLOSED_PERIOD};

/// The month a date falls in: `2026-11-18` reads as `2026-11`.
///
/// A date the engine cannot read yields nothing, so an unreadable date never matches a closed period by accident and
/// never silently escapes the check either: the posting path requires a date, and a missing one is refused there.
pub fn month_of(date: &str) -> Option<String> {
    let date = date.trim();
    let mut parts = date.split('-');
    let year = parts.next()?;
    let month = parts.next()?;
    if year.len() != 4 || month.len() != 2 {
        return None;
    }
    if !year.chars().all(|digit| digit.is_ascii_digit())
        || !month.chars().all(|digit| digit.is_ascii_digit())
    {
        return None;
    }
    let month_number: u32 = month.parse().ok()?;
    if !(1..=12).contains(&month_number) {
        return None;
    }
    Some(format!("{year}-{month}"))
}

/// A period as a person writes it: `2026-11`, and nothing else.
fn month_written_as_a_period(period: &str) -> Option<String> {
    let period = period.trim();
    if period.len() != 7 {
        return None;
    }
    month_of(&format!("{period}-01"))
}

/// A period as a person writes it, or nothing when it does not read as one.
///
/// A month is the unit everything here is stated in, so a caller that has been handed `2026-11` should not have to know how
/// the engine spells it.
pub fn period_of(period: &str) -> Option<String> {
    month_written_as_a_period(period)
}

/// The last day of a period, as the date its charge belongs to: `2026-11` reads as `2026-11-30`.
///
/// Depreciation is a charge for a month, and a month has no single day, so it is dated at the end of the month it belongs
/// to. That is also the date a reader expects to find it under in the journal.
pub fn last_day_of(period: &str) -> Option<String> {
    let period = period_of(period)?;
    let year: u32 = period[0..4].parse().ok()?;
    let month: u32 = period[5..7].parse().ok()?;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        // February has a day more in a leap year: a charge dated 29 February in a year without one would be a date that
        // does not exist
        _ if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) => {
            29
        }
        _ => 28,
    };
    Some(format!("{period}-{days:02}"))
}

/// The months this Book has closed, oldest first.
pub fn closed_months<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
) -> Result<Vec<String>, StoreError> {
    let mut months: Vec<String> = Records::new(graph, CLOSED_PERIOD)
        .list(scope, 4000)?
        .into_iter()
        .map(|record| field(&record, "period").to_string())
        .filter(|period| !period.is_empty())
        .collect();
    months.sort();
    months.dedup();
    Ok(months)
}

/// `POST /accounting/closed-periods/`: lock a month.
///
/// The day of the close is supplied by the caller rather than read from a clock, so the same request means the same
/// thing tomorrow and a test can pin it.
pub fn close_period<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let period = text(payload, "period");
    // a period is written as a month, so it is read as one: appending a day would turn the year "2026" into January,
    // which is exactly the sort of half-right answer this check exists to stop
    let Some(month) = month_written_as_a_period(&period) else {
        return Err(refused(
            "A period reads as a month: write it like 2026-11, the year and then the month.",
        ));
    };
    let closes_on = text(payload, "closes_on");
    let Some(day_of_close) = month_of(&closes_on) else {
        return Err(refused(
            "Say the day the month is being closed: closes_on, like 2026-11-18.",
        ));
    };
    if month > day_of_close {
        return Err(refused(&format!(
            "{month} has not started yet, so it cannot be closed. It can be closed once it is over."
        )));
    }
    let already = closed_months(graph, scope)?;
    if already.contains(&month) {
        return Err(refused(&format!(
            "{month} is already closed, so nothing more can be posted into it."
        )));
    }
    let closed_by = text(payload, "closed_by");
    Records::new(graph, CLOSED_PERIOD).create(
        scope,
        &json!({
            "period": month,
            "closes_on": closes_on,
            "closed_by": closed_by,
            "note": text(payload, "note"),
        }),
    )
}

/// `GET /accounting/closed-periods/`: the months closed, and where the open books begin.
///
/// `open_from` is the first day after the most recent closed month, which is the date a person needs when they are
/// about to write an entry and want to know the earliest day it can carry.
pub fn list_periods<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    let months = closed_months(graph, scope)?;
    let closed: Vec<Value> = Records::new(graph, CLOSED_PERIOD)
        .list(scope, 4000)?
        .into_iter()
        .filter(|record| !field(record, "period").is_empty())
        .map(|record| {
            json!({
                "period": field(&record, "period"),
                "closes_on": field(&record, "closes_on"),
                "closed_by": field(&record, "closed_by"),
                "note": field(&record, "note"),
            })
        })
        .collect();
    Ok(json!({
        "closed": closed,
        "closed_months": months,
        "open_from": months.last().map(|month| first_day_after(month)),
    }))
}

/// The first day of the month after `2026-11`, as a date: `2026-12-01`.
pub fn first_day_after(month: &str) -> String {
    let year: i32 = month[..4].parse().unwrap_or(0);
    let number: u32 = month[5..7].parse().unwrap_or(1);
    if number == 12 {
        format!("{:04}-01-01", year + 1)
    } else {
        format!("{:04}-{:02}-01", year, number + 1)
    }
}

/// Whether a date falls in a month this Book has closed, and the sentence to answer with when it does.
///
/// Read by the posting path before anything is written, so an entry into a closed month is refused rather than stored
/// and noticed later.
pub fn refusal_for<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    entry_date: &str,
) -> Result<Option<String>, StoreError> {
    let Some(month) = month_of(entry_date) else {
        return Ok(None);
    };
    if closed_months(graph, scope)?.contains(&month) {
        return Ok(Some(format!(
            "{month} is closed, so nothing can be posted into it. Date the entry in an open month, or reopen {month} \
             if it was closed by mistake."
        )));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn close(graph: &MemoryGraph, period: &str, closes_on: &str) -> Result<Value, StoreError> {
        close_period(
            graph,
            &scope(),
            &json!({"period": period, "closes_on": closes_on, "closed_by": "Samuel"}),
        )
    }

    #[test]
    fn a_date_reads_as_the_month_it_falls_in() {
        assert_eq!(month_of("2026-11-18").as_deref(), Some("2026-11"));
        assert_eq!(month_of("2026-01-01").as_deref(), Some("2026-01"));
        // anything that is not a date is not a month, so it can neither match nor pass by pretending
        assert_eq!(month_of(""), None);
        assert_eq!(month_of("2026"), None);
        assert_eq!(month_of("2026-13-01"), None);
        assert_eq!(month_of("november"), None);
    }

    #[test]
    fn closing_a_month_is_remembered_and_the_open_books_begin_the_day_after() {
        let graph = MemoryGraph::new();
        close(&graph, "2026-10", "2026-11-05").expect("October closes");
        close(&graph, "2026-11", "2026-12-02").expect("November closes");

        let answer = list_periods(&graph, &scope()).expect("the periods");
        assert_eq!(
            answer["closed_months"],
            json!(["2026-10", "2026-11"]),
            "the months come back in order, so the most recent one is the last"
        );
        assert_eq!(answer["open_from"], json!("2026-12-01"));
        assert_eq!(answer["closed"][1]["closed_by"], json!("Samuel"));
    }

    #[test]
    fn the_year_turns_over_without_reading_as_december_of_the_same_year() {
        let graph = MemoryGraph::new();
        close(&graph, "2026-12", "2027-01-04").expect("December closes");
        let answer = list_periods(&graph, &scope()).expect("the periods");
        assert_eq!(answer["open_from"], json!("2027-01-01"));
    }

    #[test]
    fn closing_the_same_month_twice_is_refused_rather_than_recorded_twice() {
        let graph = MemoryGraph::new();
        close(&graph, "2026-11", "2026-12-01").expect("November closes");
        let again = close(&graph, "2026-11", "2026-12-02").expect_err("and cannot close twice");
        let words = again.to_string();
        assert!(words.contains("2026-11 is already closed"), "{words}");
        // and the Book still holds one decision about it, not two
        assert_eq!(
            list_periods(&graph, &scope()).expect("periods")["closed_months"],
            json!(["2026-11"])
        );
    }

    #[test]
    fn a_month_that_has_not_started_cannot_be_closed() {
        let graph = MemoryGraph::new();
        let refused = close(&graph, "2026-12", "2026-11-18").expect_err("December has not begun");
        let words = refused.to_string();
        assert!(words.contains("has not started yet"), "{words}");
        assert!(
            list_periods(&graph, &scope()).expect("periods")["closed_months"]
                .as_array()
                .expect("months")
                .is_empty()
        );
    }

    #[test]
    fn a_period_that_is_not_a_month_is_refused_with_the_shape_to_write() {
        let graph = MemoryGraph::new();
        for wrong in ["2026", "november", "2026-13", "26-11"] {
            let refused = close(&graph, wrong, "2026-11-18").expect_err("not a month");
            assert!(
                refused.to_string().contains("write it like 2026-11"),
                "{wrong} answered {refused}"
            );
        }
    }

    #[test]
    fn a_close_without_a_day_is_refused() {
        let graph = MemoryGraph::new();
        let refused = close_period(&graph, &scope(), &json!({"period": "2026-11"}))
            .expect_err("no day of close");
        assert!(refused.to_string().contains("closes_on"), "{refused}");
    }

    #[test]
    fn an_entry_dated_in_a_closed_month_is_refused_and_the_sentence_names_the_way_out() {
        let graph = MemoryGraph::new();
        close(&graph, "2026-11", "2026-12-01").expect("November closes");

        let words = refusal_for(&graph, &scope(), "2026-11-30")
            .expect("the check")
            .expect("a refusal");
        assert!(words.contains("2026-11 is closed"), "{words}");
        assert!(words.contains("reopen 2026-11"), "{words}");

        // the same day in an open month passes, and a neighbouring month is untouched
        assert_eq!(
            refusal_for(&graph, &scope(), "2026-12-01").expect("check"),
            None
        );
        assert_eq!(
            refusal_for(&graph, &scope(), "2026-10-31").expect("check"),
            None
        );
    }

    #[test]
    fn nothing_is_closed_until_something_is_closed() {
        let graph = MemoryGraph::new();
        let answer = list_periods(&graph, &scope()).expect("periods");
        assert_eq!(answer["closed_months"], json!([]));
        assert_eq!(answer["open_from"], Value::Null);
        assert_eq!(
            refusal_for(&graph, &scope(), "2026-11-18").expect("check"),
            None
        );
    }
}
