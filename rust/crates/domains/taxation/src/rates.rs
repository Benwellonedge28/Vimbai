#![forbid(unsafe_code)]
//! What every tax service needs: which rate applies, and how a rate turns an amount into tax.

use serde_json::Value;

use vimbai_common::Money;
use vimbai_store::amounts::stored_amount;
use vimbai_store::read::field;
use vimbai_store::StoreError;

/// The rate a tax type falls back to when the caller has registered none.
///
/// The shipped service answered a VAT sale at 20% before the first rate was ever entered, so the answer
/// depended on a default table. It is kept as it was, with the same keys, because a caller that never
/// registered a rate still got an answer rather than a refusal.
pub fn default_rate(tax_type: &str) -> &'static str {
    match tax_type {
        "vat" => "20",
        "gst" => "10",
        "sales_tax" => "8",
        "withholding_tax" => "15",
        "corporate_tax" | "income_tax" => "25",
        "capital_gains_tax" => "20",
        _ => "20",
    }
}

/// The applicable rate for a tax type in a jurisdiction on a day, if the caller has registered one.
///
/// A rate covers a window: it starts on `effective_from` and, unless one was given, never ends. The first
/// rate that covers the day wins, which is what the shipped service did - it read the caller's rates in
/// the order they were stored and returned the first that matched, so a later registration does not
/// silently replace an earlier one still in force.
pub fn applicable(rows: &[Value], tax_type: &str, jurisdiction: &str, day: &str) -> Option<Value> {
    rows.iter()
        .find(|row| {
            field(row, "tax_type") == tax_type
                && field(row, "jurisdiction") == jurisdiction
                && flag_is_on(row)
                && covers(row, day)
        })
        .cloned()
}

fn flag_is_on(record: &Value) -> bool {
    record
        .get("is_active")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// Whether the rate's window covers `day`, read as calendar days.
fn covers(record: &Value, day: &str) -> bool {
    let from = field(record, "effective_from");
    if !from.is_empty() && from.get(..10).unwrap_or(from) > day {
        return false;
    }
    let to = field(record, "effective_to");
    if to.is_empty() {
        return true;
    }
    to.get(..10).unwrap_or(to) >= day
}

/// The three amounts a tax calculation produces, kept apart so both the single and batch answers agree.
pub struct Split {
    pub gross: Money,
    pub net: Money,
    pub tax: Money,
}

/// Work the tax out of an amount, either as a net figure the tax is added to or as a gross figure it is
/// taken out of.
///
/// Both directions are kept because the shipped endpoint took `is_net` and answered differently: a net
/// amount has the rate applied to it and the tax added on, while a gross amount has the tax divided back
/// out (rate/(1+rate)) so the net and tax add up to the gross exactly. Rounding to the cent happens once,
/// on the tax, and the other figure is the remainder - so the two always add back to the gross.
pub fn split(amount: &Money, percent: &Money, is_net: bool) -> Split {
    let hundred = Money::from_i64(100);
    if is_net {
        let tax = amount.times(percent).over(&hundred).round(2);
        Split {
            gross: amount.plus(&tax),
            net: amount.clone(),
            tax,
        }
    } else {
        let divisor = hundred.plus(percent);
        let tax = amount.times(percent).over(&divisor).round(2);
        Split {
            gross: amount.clone(),
            net: amount.minus(&tax),
            tax,
        }
    }
}

/// The rate percentage to use: the registered one when there is one, otherwise the default table.
pub fn percent_for(registered: Option<&Value>, tax_type: &str) -> Result<Money, StoreError> {
    if let Some(row) = registered {
        return stored_amount(row, "rate_percentage");
    }
    Money::parse(default_rate(tax_type))
        .map_err(|_| StoreError::Invalid("rate is not an amount".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn money(text: &str) -> Money {
        Money::parse(text).expect("parsed")
    }

    #[test]
    fn a_net_amount_has_the_tax_added_on() {
        let split = split(&money("100"), &money("20"), true);
        assert_eq!(split.gross.to_fixed(2), "120.00");
        assert_eq!(split.net.to_fixed(2), "100.00");
        assert_eq!(split.tax.to_fixed(2), "20.00");
    }

    #[test]
    fn a_gross_amount_has_the_tax_divided_back_out() {
        let split = split(&money("120"), &money("20"), false);
        assert_eq!(split.net.to_fixed(2), "100.00");
        assert_eq!(split.tax.to_fixed(2), "20.00");
        // the two add back to the gross exactly, which is the point of rounding only the tax
        assert_eq!(split.net.plus(&split.tax).to_fixed(2), "120.00");
    }

    #[test]
    fn an_awkward_gross_still_adds_back_to_itself() {
        let split = split(&money("99.99"), &money("20"), false);
        assert_eq!(
            split.net.plus(&split.tax).to_fixed(2),
            "99.99",
            "the split did not add back up"
        );
    }

    #[test]
    fn the_default_table_answers_before_a_rate_is_registered() {
        assert_eq!(default_rate("vat"), "20");
        assert_eq!(default_rate("gst"), "10");
        assert_eq!(default_rate("something_new"), "20");
    }

    #[test]
    fn a_rate_covers_its_window_and_not_a_day_before_it() {
        let rate = json!({
            "tax_type": "vat",
            "jurisdiction": "ZW",
            "is_active": true,
            "rate_percentage": "15",
            "effective_from": "2026-01-01T00:00:00",
        });
        let rows = vec![rate];
        assert!(applicable(&rows, "vat", "ZW", "2026-03-01").is_some());
        assert!(applicable(&rows, "vat", "ZW", "2025-12-31").is_none());
        assert!(applicable(&rows, "vat", "ZA", "2026-03-01").is_none());
    }

    #[test]
    fn a_rate_that_ended_no_longer_applies() {
        let rows = vec![json!({
            "tax_type": "vat",
            "jurisdiction": "ZW",
            "is_active": true,
            "rate_percentage": "15",
            "effective_from": "2025-01-01",
            "effective_to": "2025-12-31",
        })];
        assert!(applicable(&rows, "vat", "ZW", "2025-06-30").is_some());
        assert!(applicable(&rows, "vat", "ZW", "2026-01-01").is_none());
    }

    #[test]
    fn a_rate_switched_off_is_not_picked() {
        let rows = vec![json!({
            "tax_type": "vat",
            "jurisdiction": "ZW",
            "is_active": false,
            "rate_percentage": "15",
            "effective_from": "2025-01-01",
        })];
        assert!(applicable(&rows, "vat", "ZW", "2026-03-01").is_none());
    }
}
