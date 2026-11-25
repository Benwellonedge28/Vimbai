#![forbid(unsafe_code)]
//! Rules that keep a business's own words out of the engine.
//!
//! ADR-014 says Vimbai fits every business because only the laws of accounting are code: an account's number and name,
//! its currency, its tax and its calendar are the Book's data. A number written in the engine is the clearest way to
//! break that promise, because it does not fail: it posts a business's money to whatever account happens to carry that
//! number, or to nothing at all, and the entry still balances.
//!
//! The engine may name a number in exactly one place: the chart a Book opens with, which is data written as code.
//! Everything else must ask the Book for its accounts.
//!
//! What is left is a backlog, not a permission. Each file that still carries numbers is listed with how many, and the
//! list is a ratchet: a file that gains a number fails, a file that is not listed fails, and a file that drops below its
//! figure fails until the figure is lowered. So the backlog can only shrink, and every fix has to be recorded.

use std::path::{Path, PathBuf};

/// The one file allowed to write an account number: the chart a Book opens with.
pub const THE_STARTING_CHART: &str = "crates/domains/accounting/src/chart.rs";

/// Files that still carry a number, and how many lines of them. Lower a figure when a fix lands; never raise one.
pub const STILL_NUMBERED: &[(&str, usize)] = &[
    (
        "crates/calculations/equity_payroll/src/partnership_changes.rs",
        7,
    ),
    (
        "crates/calculations/equity_payroll/src/partnership_closure.rs",
        4,
    ),
    (
        "crates/calculations/equity_payroll/src/share_capital.rs",
        10,
    ),
    (
        "crates/calculations/equity_payroll/src/share_registry.rs",
        3,
    ),
    ("crates/calculations/treasury_fx/src/cash.rs", 4),
    ("crates/domains/accounting/src/bookkeeping.rs", 7),
    ("crates/domains/accounting/src/expenses.rs", 1),
    ("crates/domains/accounting/src/fx_revaluation.rs", 2),
    ("crates/domains/assets/src/disposal_account.rs", 5),
    ("crates/domains/assets/src/revaluation_reserve.rs", 8),
    ("crates/domains/banking/src/reconciliation.rs", 1),
    ("crates/domains/equity/src/capital_reconstruction.rs", 2),
    ("crates/domains/equity/src/debentures.rs", 11),
    ("crates/domains/equity/src/issued_share_capital.rs", 3),
    ("crates/domains/equity/src/redemption_reserve.rs", 10),
    ("crates/domains/equity/src/reserves.rs", 8),
    ("crates/domains/equity/src/retained_profits.rs", 8),
    ("crates/domains/equity/src/share_events.rs", 9),
    ("crates/domains/equity/src/share_premium.rs", 10),
    ("crates/domains/equity/src/share_redemption.rs", 7),
    ("crates/domains/equity/src/shares.rs", 14),
    ("crates/domains/partnerships/src/changes.rs", 7),
    ("crates/domains/partnerships/src/revaluation.rs", 4),
    ("crates/domains/receivables/src/provision.rs", 2),
    ("crates/domains/treasury/src/foreign_exchange.rs", 1),
    ("crates/platform/events/src/broker.rs", 1),
    ("crates/platform/integrations/src/graphql.rs", 6),
];

/// Every file, and how many of its lines write a four-digit account number outside a test.
///
/// A test may write any number it likes: it is standing in for a Book, not posting to one. So the sweep stops where a
/// file's tests begin, which is what makes the figures mean something.
pub fn numbers_by_file(root: &Path) -> Vec<(String, usize)> {
    let mut counted = Vec::new();
    for path in rust_files(&root.join("rust/crates")) {
        let relative = path
            .strip_prefix(root.join("rust"))
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        if relative == THE_STARTING_CHART
            || relative.contains("/tests/")
            || relative.ends_with("tests.rs")
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let code = &text[..text.find("#[cfg(test)]").unwrap_or(text.len())];
        let count = code
            .lines()
            .filter(|line| !line.trim_start().starts_with("//") && writes_a_number(line))
            .count();
        if count > 0 {
            counted.push((relative, count));
        }
    }
    counted.sort();
    counted
}

/// Whether a line writes a number that could be an account's.
///
/// Four digits in quotes is how every account number in this repository is written, so the rule needs no list of what an
/// account is called. The one thing that looks the same is a year, which is why a figure between 1900 and 2100 that is
/// not a round hundred is read as a year: an account numbered 2026 in the two-thousands is the one shape this rule cannot
/// tell from the year 2026, and the journals in the backlog are numbered in hundreds, so nothing is missed by saying so
/// plainly rather than accusing every line that writes a date.
fn writes_a_number(line: &str) -> bool {
    let bytes: Vec<char> = line.chars().collect();
    let mut index = 0;
    while index + 5 < bytes.len() {
        if bytes[index] == '"'
            && bytes[index + 5] == '"'
            && bytes[index + 1..index + 5].iter().all(char::is_ascii_digit)
        {
            let digits: String = bytes[index + 1..index + 5].iter().collect();
            let value: u32 = digits.parse().unwrap_or(0);
            let is_a_year = (1900..=2100).contains(&value) && !value.is_multiple_of(100);
            if !is_a_year {
                return true;
            }
        }
        index += 1;
    }
    false
}

fn rust_files(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(rust_files(&path));
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            found.push(path);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_written_in_quotes_is_seen_and_a_date_is_not() {
        assert!(writes_a_number(r#"json!({"account_number": "4100"})"#));
        assert!(writes_a_number(r#"line("3300", "Retained Earnings")"#));
        assert!(!writes_a_number(r#"{"date": "2026-11-25"}"#));
        assert!(!writes_a_number(r#"{"year": "2026"}"#));
        assert!(!writes_a_number(r#"{"period": "2026-11"}"#));
        // a round hundred is how every journal in the backlog is numbered, so it stays a number
        assert!(writes_a_number(r#"line("2000", "Accounts Payable")"#));
        assert!(!writes_a_number(r#"{"amount": "12345"}"#));
    }
}
