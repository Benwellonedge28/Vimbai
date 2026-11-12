//! Divestiture accounting, ported from divestiture-service.
//!
//! Every figure is a decimal, so a disposal of a large carrying value keeps the cent
//! that matters in the gain.
//!
//! The service's arithmetic is kept as written, including one consequence that looks
//! like an omission: when a retained interest exists, its fair value is *added* to the
//! gain while the carrying value is still derecognised in full. The journal entries
//! therefore do not balance in that case. Pinned so the change is visible if anyone
//! decides to correct it.

use serde_json::{json, Value};
use vimbai_common::money::amount_of as money_of;
use vimbai_common::Money;

/// How Python renders a float in an f-string: `100.0`, not `100`, and `33.33` when
/// the value is not whole. The journal text is part of the published output.
fn python_number(value: &Money) -> String {
    let rounded = value.round(2);
    let text = rounded.to_fixed(2);
    let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        format!("{whole}.0")
    } else {
        format!("{whole}.{fraction}")
    }
}

/// The disposal's gain and the entries that record it.
pub fn evaluate(payload: &Value) -> Result<Value, String> {
    let proceeds = money_of(payload.get("disposal_proceeds"))?;
    let costs = money_of(payload.get("disposal_costs"))?;
    let carrying = money_of(payload.get("carrying_value"))?;
    let goodwill = money_of(payload.get("associated_goodwill"))?;
    let retained_fv = money_of(payload.get("retained_interest_fair_value"))?;
    let retained_pct = money_of(payload.get("retained_interest_pct"))?;

    let net_proceeds = proceeds.minus(&costs);
    let gain_loss = net_proceeds.minus(&carrying.minus(&goodwill));

    let keeps_interest = retained_pct > Money::zero() && retained_fv > Money::zero();

    let (total_gain, mut entries) = if keeps_interest {
        let gain = gain_loss.plus(&retained_fv);
        (
            gain.clone(),
            vec![
                format!("Dr Cash/Bank ({})", python_number(&net_proceeds)),
                format!(
                    "Dr Investment in Subsidiary (retained) ({})",
                    python_number(&retained_fv)
                ),
                format!("Cr Investment in Subsidiary ({})", python_number(&carrying)),
                gain_line(&gain),
                if goodwill > Money::zero() {
                    "Cr Goodwill derecognized".to_string()
                } else {
                    String::new()
                },
            ],
        )
    } else {
        (
            gain_loss.clone(),
            vec![
                format!("Dr Cash/Bank ({})", python_number(&net_proceeds)),
                format!("Cr Investment in Subsidiary ({})", python_number(&carrying)),
                gain_line(&gain_loss),
                if goodwill > Money::zero() {
                    "Dr/Cr Goodwill adjustment".to_string()
                } else {
                    String::new()
                },
            ],
        )
    };
    entries.retain(|line| !line.is_empty());

    Ok(json!({
        "company_id": payload.get("company_id").and_then(Value::as_str).unwrap_or(""),
        "subsidiary_name": payload.get("subsidiary_name").and_then(Value::as_str).unwrap_or(""),
        "disposal_method": payload.get("disposal_method").and_then(Value::as_str).unwrap_or(""),
        "carrying_value": carrying.round(2).to_fixed(2),
        "net_proceeds": net_proceeds.round(2).to_fixed(2),
        "gain_or_loss": gain_loss.round(2).to_fixed(2),
        "retained_interest_fv": retained_fv.round(2).to_fixed(2),
        "total_gain_recognized": total_gain.round(2).to_fixed(2),
        "goodwill_derecognized": goodwill.round(2).to_fixed(2),
        "journal_entries": entries,
    }))
}

fn gain_line(total_gain: &Money) -> String {
    if *total_gain > Money::zero() {
        format!("Cr Gain on Disposal ({})", python_number(total_gain))
    } else {
        format!("Dr Loss on Disposal ({})", python_number(&total_gain.abs()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(proceeds: &str, costs: &str, carrying: &str) -> Value {
        json!({
            "company_id": "c1",
            "subsidiary_name": "Sub",
            "disposal_method": "sale",
            "disposal_proceeds": proceeds,
            "disposal_costs": costs,
            "carrying_value": carrying,
            "associated_goodwill": "0",
            "retained_interest_pct": "0",
            "retained_interest_fair_value": "0",
        })
    }

    #[test]
    fn a_plain_sale_book_the_gain_and_its_entry() {
        let out = evaluate(&payload("1000000", "50000", "800000")).unwrap();
        assert_eq!(out["net_proceeds"], "950000.00");
        assert_eq!(out["gain_or_loss"], "150000.00");
        assert_eq!(out["total_gain_recognized"], "150000.00");
        assert_eq!(out["journal_entries"][0], "Dr Cash/Bank (950000.0)");
        assert_eq!(out["journal_entries"][2], "Cr Gain on Disposal (150000.0)");
    }

    #[test]
    fn a_loss_is_booked_as_a_debit() {
        let out = evaluate(&payload("500000", "0", "800000")).unwrap();
        assert_eq!(out["gain_or_loss"], "-300000.00");
        assert_eq!(out["journal_entries"][2], "Dr Loss on Disposal (300000.0)");
    }

    #[test]
    fn a_retained_interest_is_added_without_reducing_the_carrying_value() {
        let mut body = payload("1000000", "0", "800000");
        body["retained_interest_pct"] = json!("20");
        body["retained_interest_fair_value"] = json!("250000");
        let out = evaluate(&body).unwrap();
        assert_eq!(out["gain_or_loss"], "200000.00");
        assert_eq!(out["total_gain_recognized"], "450000.00");
        assert_eq!(
            out["journal_entries"][1],
            "Dr Investment in Subsidiary (retained) (250000.0)"
        );
    }

    #[test]
    fn a_large_disposal_keeps_its_cent() {
        let out = evaluate(&payload(
            "123456789012345678901234.56",
            "0.01",
            "100000000000000000000000",
        ))
        .unwrap();
        assert_eq!(out["net_proceeds"], "123456789012345678901234.55");
        assert_eq!(out["gain_or_loss"], "23456789012345678901234.55");
    }

    #[test]
    fn cents_render_the_way_python_printed_them() {
        let out = evaluate(&payload("100.50", "0.20", "50.00")).unwrap();
        assert_eq!(out["journal_entries"][0], "Dr Cash/Bank (100.3)");
        assert_eq!(out["gain_or_loss"], "50.30");
    }
}
