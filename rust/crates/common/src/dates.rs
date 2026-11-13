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

/// The date a count of days from 1970-01-01 lands on, written `YYYY-MM-DD`.
///
/// The inverse of [`day_of`], so a due date thirty days out can be written down rather than only measured.
pub fn date_of(days: i64) -> String {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

/// Days after a date, written back as a date: `30` days after `2026-01-31` is `2026-03-02`.
pub fn date_after(date: &str, days: i64) -> Option<String> {
    Some(date_of(day_of(date)? + days))
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
    fn a_day_count_written_back_is_the_date_it_lands_on() {
        assert_eq!(date_of(day_of("2026-01-01").unwrap()), "2026-01-01");
        assert_eq!(date_of(day_of("2024-02-29").unwrap()), "2024-02-29");
        assert_eq!(date_after("2024-02-28", 1).as_deref(), Some("2024-02-29"));
        assert_eq!(date_after("2026-01-31", 30).as_deref(), Some("2026-03-02"));
        assert_eq!(date_after("2026-12-15", 30).as_deref(), Some("2027-01-14"));
        for day in [-20000i64, -1, 0, 1, 5000, 21000] {
            assert_eq!(day_of(&date_of(day)), Some(day));
        }
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

/// The instant a moment was at, in seconds from 1970-01-01T00:00:00Z.
///
/// The rest of this module counts whole days, which is right for ageing a debtor and wrong for a till's takings: two
/// transactions on one afternoon are hours apart, and a report asked for a span has to be able to say so. This reads the
/// same dates with their clock, and accepts either form a caller may send:
///
/// * a number, which is already an instant - the engine stamps its own moments that way;
/// * text, `YYYY-MM-DD` on its own or with `THH:MM:SS`, and with `Z`, a `+HH:MM` / `-HH:MM` offset, or neither, in which
///   case the clock is read as written rather than being moved.
///
/// A moment that cannot be read has no instant, so a caller can tell a missing bound from the epoch.
pub fn instant_of(value: &str) -> Option<f64> {
    let text = value.trim();
    if let Ok(seconds) = text.parse::<f64>() {
        return Some(seconds);
    }
    let day = day_of(text)?;
    let behind = text.get(10..).unwrap_or("").trim_start_matches(['T', ' ']);
    // an offset is honoured rather than ignored: +02:00 is two hours EARLIER in UTC
    let (clock, offset) = split_offset(behind);
    let mut parts = clock.split(':');
    let hours: i64 = parts.next().unwrap_or("0").trim().parse().unwrap_or(0);
    let minutes: i64 = parts.next().unwrap_or("0").trim().parse().unwrap_or(0);
    let seconds: f64 = parts.next().unwrap_or("0").trim().parse().unwrap_or(0.0);
    if !(0..=24).contains(&hours) || !(0..=59).contains(&minutes) {
        return None;
    }
    Some(day as f64 * 86_400.0 + (hours * 3_600 + minutes * 60) as f64 + seconds - offset as f64)
}

/// The clock part and the offset behind it, in seconds.
fn split_offset(behind: &str) -> (&str, i64) {
    let trimmed = behind.trim_end_matches('Z');
    let Some(at) = trimmed.rfind(['+', '-']) else {
        return (trimmed, 0);
    };
    // a lone date has no offset, and a negative sign that starts the text is not one either
    if at == 0 {
        return (trimmed, 0);
    }
    let offset = trimmed.get(at..).and_then(|tail| {
        let (hours, minutes) = tail.get(1..)?.split_once(':')?;
        let hours: i64 = hours.trim().parse().ok()?;
        let minutes: i64 = minutes.trim().parse().ok()?;
        let sign = if tail.starts_with('-') { -1 } else { 1 };
        Some(sign * (hours * 3_600 + minutes * 60))
    });
    (trimmed.get(..at).unwrap_or(trimmed), offset.unwrap_or(0))
}

#[cfg(test)]
mod instant_tests {
    use super::*;

    #[test]
    fn an_instant_is_seconds_and_text_reads_to_the_same_second() {
        // a number is already an instant
        assert_eq!(instant_of("1000"), Some(1000.0));
        // the epoch itself, written either way
        assert_eq!(instant_of("1970-01-01"), Some(0.0));
        assert_eq!(instant_of("1970-01-02T00:00:00Z"), Some(86_400.0));
        assert_eq!(instant_of("1970-01-01T01:30:15Z"), Some(5_415.0));
        // an offset moves the moment back to UTC rather than being ignored
        assert_eq!(instant_of("1970-01-01T02:00:00+02:00"), Some(0.0));
        assert_eq!(instant_of("1970-01-01T00:00:00-01:00"), Some(3_600.0));
        // two takings on one afternoon are NOT a day apart, which is the whole reason this exists
        assert_eq!(
            instant_of("2026-11-13T14:00:00Z"),
            Some(instant_of("2026-11-13T09:00:00Z").unwrap() + 18_000.0)
        );
        // a moment that cannot be read has no instant
        assert_eq!(instant_of("not a date"), None);
        assert_eq!(instant_of("2026-13-13"), None);
    }
}

/// The moment a count of seconds lands on, written `YYYY-MM-DDTHH:MM:SSZ`.
///
/// The inverse of [`instant_of`], so a figure the engine stamps as seconds can be handed to a caller that speaks dates.
/// Whole seconds only: a fraction is dropped rather than rounded, because rounding a moment forward can move it into the
/// next day at the end of a year.
pub fn written_at(seconds: f64) -> String {
    let whole = seconds.floor() as i64;
    let day = whole.div_euclid(86_400);
    let into_day = whole.rem_euclid(86_400);
    format!(
        "{}T{:02}:{:02}:{:02}Z",
        date_of(day),
        into_day / 3_600,
        (into_day % 3_600) / 60,
        into_day % 60
    )
}

#[cfg(test)]
mod written_tests {
    use super::*;

    #[test]
    fn a_moment_is_written_back_as_the_instant_it_came_from() {
        assert_eq!(written_at(0.0), "1970-01-01T00:00:00Z");
        assert_eq!(written_at(5_415.0), "1970-01-01T01:30:15Z");
        // a fraction is dropped, never rounded into the next day
        assert_eq!(written_at(86_399.9), "1970-01-01T23:59:59Z");
        // and it reads back to itself
        for seconds in [0.0, 1_000.0, 1_790_000_000.0] {
            assert_eq!(instant_of(&written_at(seconds)), Some(seconds));
        }
    }
}
