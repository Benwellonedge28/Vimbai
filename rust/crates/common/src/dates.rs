#![forbid(unsafe_code)]
//! Calendar days, counted rather than subtracted.
//!
//! Four places in the workspace needed to know how many days lie between two business dates: the
//! subsidiary journals ageing a control account, treasury ageing a cash flow, the ledger kernel ageing
//! receivables, and the receivables provisions ageing a debtor. Each of them wrote the same closed-form
//! count out again, so a leap year fixed in one was not fixed in the others. The count lives here once.
//!
//! Dates arrive as text - `2026-01-31`, sometimes with a time and an offset behind it - and every one of
//! those services read the calendar day as written and ignored the clock, which is what happens here: the
//! day part is taken and the days are counted from it. A date that cannot be read has no day, so a caller
//! can tell a missing date from the epoch rather than being handed a silent zero.

/// The day part of a date written `YYYY-MM-DD...`, as a count of days from 1970-01-01.
pub fn day_of(value: &str) -> Option<i64> {
    let text = value.trim();
    let date = text.get(..10)?;
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(civil_day(year, month, day))
}

/// Days from `earlier` to `later`, both written as dates. Negative when `later` is the first of the two.
pub fn days_between(later: &str, earlier: &str) -> Option<i64> {
    Some(day_of(later)? - day_of(earlier)?)
}

/// The same day arithmetic the calendar uses, in closed form (no leap-year tables to get wrong).
fn civil_day(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_are_counted_the_way_a_calendar_counts_them() {
        assert_eq!(days_between("2026-02-01", "2026-01-01"), Some(31));
        assert_eq!(days_between("2024-03-01", "2024-02-01"), Some(29));
        assert_eq!(days_between("2027-01-01", "2026-01-01"), Some(365));
        assert_eq!(days_between("2028-03-01", "2028-02-29"), Some(1));
        assert_eq!(days_between("2026-01-01", "2026-02-01"), Some(-31));
        assert_eq!(days_between("2026-01-01", "2026-01-01"), Some(0));
    }

    #[test]
    fn a_timestamp_is_read_as_its_calendar_day() {
        assert_eq!(day_of("2026-01-31T23:59:00+02:00"), day_of("2026-01-31"));
    }

    #[test]
    fn something_that_is_not_a_date_has_no_day() {
        assert_eq!(day_of(""), None);
        assert_eq!(day_of("not a date"), None);
        assert_eq!(day_of("2026-13-01"), None);
    }
}
