//! ISO-8601 rendering in UTC, matching the strings the service emitted.
//!
//! Instants are microseconds since the Unix epoch. The caller supplies them, so
//! the crate never reads a clock and a session's expiry is reproducible.

pub const MICROSECONDS_PER_HOUR: i64 = 3_600 * MICROS_PER_SECOND;

const MICROS_PER_SECOND: i64 = 1_000_000;
const MICROS_PER_DAY: i64 = 86_400 * MICROS_PER_SECOND;

/// Days since 1970-01-01 to a proleptic Gregorian civil date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `2026-11-03T16:18:26.360941+00:00`, or without the fraction when it is zero,
/// which is what Python's `isoformat()` produced.
pub fn to_iso(micros: i64) -> String {
    let days = micros.div_euclid(MICROS_PER_DAY);
    let remainder = micros.rem_euclid(MICROS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    let seconds = remainder / MICROS_PER_SECOND;
    let fraction = remainder % MICROS_PER_SECOND;
    let hour = seconds / 3_600;
    let minute = (seconds % 3_600) / 60;
    let second = seconds % 60;
    let stamp = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}");
    if fraction == 0 {
        format!("{stamp}+00:00")
    } else {
        format!("{stamp}.{fraction:06}+00:00")
    }
}

/// Parse the forms the service accepted: `...+00:00`, `...Z`, and a `YYYY-MM-DD`
/// date-only check that it also tolerated through `fromisoformat`.
pub fn parse_iso(value: &str) -> Option<i64> {
    let text = value.trim();
    let text = text.strip_suffix('Z').unwrap_or(text);
    let text = text.strip_suffix("+00:00").unwrap_or(text);
    let (date, rest) = text.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() {
        return None;
    }
    let (clock, fraction) = match rest.split_once('.') {
        Some((clock, fraction)) => (clock, fraction),
        None => (rest, ""),
    };
    let mut clock_parts = clock.split(':');
    let hour: i64 = clock_parts.next()?.parse().ok()?;
    let minute: i64 = clock_parts.next()?.parse().ok()?;
    let second: i64 = clock_parts.next()?.parse().ok()?;
    let fraction_micros: i64 = if fraction.is_empty() {
        0
    } else {
        let padded = format!("{:0<6}", fraction);
        padded[..6].parse().ok()?
    };
    let days = days_from_civil(year, month, day);
    Some(
        days * MICROS_PER_DAY
            + (hour * 3_600 + minute * 60 + second) * MICROS_PER_SECOND
            + fraction_micros,
    )
}

/// Inverse of `civil_from_days`.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
