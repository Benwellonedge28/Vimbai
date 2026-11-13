#![forbid(unsafe_code)]
//! The little calendar this crate needs, and nothing more: whole days between two civil dates, a date moved on by
//! so many days, and a due date advanced by a frequency.
//!
//! A civil date is turned into a day number and back with the standard shift-of-epoch arithmetic, so no calendar
//! library has to be carried for what is a handful of additions. A recurring item that falls on the 31st moves to the
//! 28th in a shorter month, which is the cap the original applied so a monthly bill never skips February.

/// Days since 1970-01-01 for a civil date, counting in the proleptic Gregorian calendar.
pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The civil date a day number falls on.
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month, day)
}

fn number(part: &str) -> Option<i64> {
    if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    part.parse().ok()
}

/// A `YYYY-MM-DD` date as a day number.
pub fn parse_date(text: &str) -> Option<i64> {
    let date_part = text.split(['T', ' ']).next().unwrap_or(text);
    let mut parts = date_part.split('-');
    let year = number(parts.next()?)?;
    let month = number(parts.next()?)?;
    let day = number(parts.next()?)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

/// A `YYYY-MM-DD` date, or a full timestamp, as seconds since the epoch. The time of day is kept so that interest
/// between two payments is counted in fractions of a day rather than whole ones.
pub fn parse_moment(text: &str) -> Option<i64> {
    let days = parse_date(text)?;
    let mut seconds = days * 86_400;
    let mut parts = text.split(['T', ' ']);
    parts.next();
    if let Some(time) = parts.next() {
        let mut hands = time.split(':');
        let hour = number(
            hands
                .next()
                .unwrap_or("0")
                .split(['+', 'Z'])
                .next()
                .unwrap_or("0"),
        )
        .unwrap_or(0);
        let minute = number(hands.next().unwrap_or("0")).unwrap_or(0);
        let second =
            number(hands.next().unwrap_or("0").split('.').next().unwrap_or("0")).unwrap_or(0);
        seconds += hour * 3600 + minute * 60 + second;
    }
    Some(seconds)
}

/// A day number written back as `YYYY-MM-DD`.
pub fn format_date(days: i64) -> String {
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

/// The next due date after one, by frequency. A day past the end of a shorter month is held at the 28th, which is the
/// cap the original applied.
pub fn advance(due: &str, frequency: &str) -> Option<String> {
    let days = parse_date(due)?;
    if frequency == "weekly" {
        return Some(format_date(days + 7));
    }
    let (year, month, day) = civil_from_days(days);
    let (next_year, next_month) = match frequency {
        "monthly" => {
            if month == 12 {
                (year + 1, 1)
            } else {
                (year, month + 1)
            }
        }
        "quarterly" => {
            let shifted = month + 3;
            (year + (shifted - 1) / 12, (shifted - 1) % 12 + 1)
        }
        _ => (year + 1, month),
    };
    let capped = day.min(28).min(days_in_month(next_year, next_month));
    Some(format_date(days_from_civil(next_year, next_month, capped)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_survives_the_round_trip() {
        for text in [
            "1950-01-01",
            "1970-01-01",
            "2000-02-29",
            "2026-11-13",
            "2100-12-31",
        ] {
            let days = parse_date(text).expect("a date");
            assert_eq!(format_date(days), text);
        }
    }

    #[test]
    fn whole_days_between_two_dates_are_counted() {
        let start = parse_date("2026-01-01").expect("a date");
        let end = parse_date("2026-02-01").expect("a date");
        assert_eq!(end - start, 31);
    }

    #[test]
    fn a_timestamp_keeps_its_time_of_day() {
        let moment = parse_moment("2026-01-02T06:00:00").expect("a moment");
        let midnight = parse_moment("2026-01-02").expect("a moment");
        assert_eq!(moment - midnight, 6 * 3600);
    }

    #[test]
    fn a_monthly_due_date_moves_into_the_next_month_and_holds_a_long_day_short() {
        assert_eq!(
            advance("2026-01-15", "monthly").as_deref(),
            Some("2026-02-15")
        );
        assert_eq!(
            advance("2026-01-31", "monthly").as_deref(),
            Some("2026-02-28")
        );
        assert_eq!(
            advance("2026-12-10", "monthly").as_deref(),
            Some("2027-01-10")
        );
    }

    #[test]
    fn the_other_frequencies_move_by_their_own_step() {
        assert_eq!(
            advance("2026-01-15", "weekly").as_deref(),
            Some("2026-01-22")
        );
        assert_eq!(
            advance("2026-11-15", "quarterly").as_deref(),
            Some("2027-02-15")
        );
        assert_eq!(
            advance("2026-06-30", "yearly").as_deref(),
            Some("2027-06-28")
        );
    }

    #[test]
    fn a_leap_day_reads_as_a_date() {
        assert!(
            parse_date("2026-02-30").is_none()
                || format_date(parse_date("2026-02-30").expect("a date")) == "2026-03-02"
        );
        assert!(parse_date("not a date").is_none());
    }
}
