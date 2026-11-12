#![forbid(unsafe_code)]
//! revaluation-reserve-service's records, on the store.
//!
//! A revaluation moves an asset's carrying amount and parks the difference in the revaluation
//! reserve, so this module keeps three records that must agree: the entry itself, the running
//! cumulative reserve for the asset, and each utilisation of that reserve.
//!
//! The figures are `Money`. The service computed gains, losses and reserves as floats, which is the
//! one place a revaluation reserve must not drift: it is a balance that is read back years later.
//!
//! The journal entries are built here and returned with the answer, but this door does not post
//! them. The Python service called the accounting service over HTTP from inside the request; a
//! ledger record belongs to the ledger, and a calculation door that posts on its own cannot be
//! retried safely. The payload is exact and complete (`journal_entry`), so the caller applies it
//! where the ledger is written, or not at all.
//!
//! Kept as shipped:
//!
//! * A rise is a gain and anything else is a loss, so an unchanged valuation records a zero loss.
//! * The net effect is gain less loss less the depreciation adjustment, which can be negative.
//! * The asset account is 1500 for `property` and 1600 for everything else; the reserve is 3320.
//! * `asset_disposal` and `transfer_retained_earnings` both credit 3300 retained earnings, and only
//!   `impairment` uses 1650, so the two reserve movers are indistinguishable in the ledger.
//! * A cumulative record carries `total_utilized` and folds it into the reserve, but nothing in this
//!   service ever raises it: recording a utilisation does not touch the cumulative record. The
//!   reserve therefore only falls by utilizations in the summary, which computes its own total.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount, amount_or_zero, stored_amount, wire};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, CUMULATIVE_REVALUATION, REVALUATION_ENTRY,
    REVALUATION_UTILIZATION,
};

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn required(payload: &Value, key: &str) -> Result<String, StoreError> {
    let value = text(payload, key);
    if value.trim().is_empty() {
        return Err(StoreError::Invalid(format!("{key} is required")));
    }
    Ok(value)
}

fn asset_account(asset_class: &str) -> &'static str {
    if asset_class == "property" {
        "1500"
    } else {
        "1600"
    }
}

fn line(account: &str, description: &str, debit: &Money, credit: &Money) -> Value {
    json!({
        "account_code": account,
        "description": description,
        "debit": debit.round(2).to_fixed(2),
        "credit": credit.round(2).to_fixed(2),
    })
}

/// `/revaluations/record`: the entry, the reserve movement and the cumulative update.
pub fn record_revaluation<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let asset_id = required(payload, "asset_id")?;
    let asset_name = required(payload, "asset_name")?;
    let asset_class = required(payload, "asset_class")?;
    let revaluation_date = required(payload, "revaluation_date")?;
    let previous = amount(payload, "previous_value")?;
    let new_value = amount(payload, "new_value")?;
    let adjustment = amount_or_zero(payload, "depreciation_adjustment")?;

    let risen = new_value.to_f64_lossy() > previous.to_f64_lossy();
    let gain = if risen {
        new_value.minus(&previous)
    } else {
        Money::zero()
    };
    let loss = if risen {
        Money::zero()
    } else {
        previous.minus(&new_value)
    };
    let net_effect = gain.minus(&loss).minus(&adjustment);

    let account = asset_account(&asset_class);
    let (description, entries) = if !gain.is_zero() {
        (
            format!("Revaluation of {asset_name}"),
            vec![
                line(account, &asset_name, &gain, &Money::zero()),
                line("3320", "Revaluation Reserve", &Money::zero(), &gain),
            ],
        )
    } else {
        (
            format!("Revaluation of {asset_name} (downward)"),
            vec![
                line("3320", "Revaluation Reserve", &loss, &Money::zero()),
                line(account, &asset_name, &Money::zero(), &loss),
            ],
        )
    };

    let company_id = required(payload, "company_id")?;
    let entry_fields = json!({
            "company_id": company_id,
            "asset_id": asset_id,
            "asset_name": asset_name,
            "asset_class": asset_class,
            "revaluation_date": revaluation_date,
            "previous_value": wire(&previous),
            "new_value": wire(&new_value),
            "revaluation_gain": wire(&gain),
            "revaluation_loss": wire(&loss),
            "depreciation_adjustment": wire(&adjustment),
            "net_effect": wire(&net_effect),
            "status": "completed",
    });
    let stored_entry = Records::new(graph, REVALUATION_ENTRY).create(scope, &entry_fields)?;
    let entry = {
        let mut answer = entry_fields.clone();
        answer["id"] = stored_entry.get("id").cloned().unwrap_or(Value::Null);
        answer
    };
    // the running reserve for this asset
    let cumulative = cumulative_for(graph, scope, &asset_id)?;
    let (total_gain, total_loss) = match &cumulative {
        Some(record) => (
            stored_amount(record, "total_revaluation_gain")?.plus(&gain),
            stored_amount(record, "total_revaluation_loss")?.plus(&loss),
        ),
        None => (gain.clone(), loss.clone()),
    };
    let utilized = match &cumulative {
        Some(record) => stored_amount(record, "total_utilized")?,
        None => Money::zero(),
    };
    let reserve = total_gain.minus(&total_loss).minus(&utilized);
    let cumulative_fields = json!({
        "company_id": company_id,
        "asset_id": asset_id,
        "total_revaluation_gain": wire(&total_gain),
        "total_revaluation_loss": wire(&total_loss),
        "total_utilized": wire(&utilized),
        "net_revaluation_reserve": wire(&reserve),
        "last_revaluation_date": revaluation_date,
    });

    let saved = match &cumulative {
        Some(record) => {
            let id = record.get("id").and_then(Value::as_str).unwrap_or_default();
            Records::new(graph, CUMULATIVE_REVALUATION).update(
                scope,
                id,
                &json!({
                    "total_revaluation_gain": wire(&total_gain),
                    "total_revaluation_loss": wire(&total_loss),
                    "total_utilized": wire(&utilized),
                    "net_revaluation_reserve": wire(&reserve),
                    "last_revaluation_date": revaluation_date,
                }),
            )
        }
        None => Records::new(graph, CUMULATIVE_REVALUATION).create(scope, &cumulative_fields),
    }?;

    // answer with the figures this call computed, not with the store's echo of the node
    let cumulative_answer = {
        let mut answer = cumulative_fields.clone();
        if let Some(id) = saved.get("id") {
            answer["id"] = id.clone();
        }
        answer
    };

    let reference = format!(
        "REV-{}",
        entry
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .take(8)
            .collect::<String>()
    );

    Ok(json!({
        "revaluation": entry,
        "cumulative": cumulative_answer,
        "journal_entry": {
            "date": revaluation_date,
            "description": description,
            "entries": entries,
            "reference": reference,
        },
    }))
}

/// `/utilizations/record`: move reserve out, and build the journal for it.
pub fn record_utilization<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let amount_moved = amount(payload, "amount")?;
    if amount_moved.to_f64_lossy() < 0.0 {
        return Err(StoreError::Invalid("amount cannot be negative".to_string()));
    }
    let utilization_type = required(payload, "utilization_type")?;
    let description = text(payload, "description");
    let utilization_date = {
        let value = text(payload, "utilization_date");
        if value.is_empty() {
            "now".to_string()
        } else {
            value
        }
    };

    let (label, credit_account) = match utilization_type.as_str() {
        "asset_disposal" => (
            format!("Revaluation reserve on asset disposal: {description}"),
            ("3300", "Retained Earnings"),
        ),
        "impairment" => (
            format!("Revaluation reserve impairment: {description}"),
            ("1650", "Impairment Loss"),
        ),
        _ => (
            format!("Transfer to retained earnings: {description}"),
            ("3300", "Retained Earnings"),
        ),
    };

    let related = text(payload, "related_asset_id");
    let mut fields = json!({
        "company_id": required(payload, "company_id")?,
        "amount": wire(&amount_moved),
        "utilization_type": utilization_type,
        "description": description,
        "utilization_date": utilization_date,
    });
    if !related.is_empty() {
        fields["related_asset_id"] = json!(related);
    }

    let saved = Records::new(graph, REVALUATION_UTILIZATION).create(scope, &fields)?;
    let reference = format!(
        "REV-UTIL-{}",
        saved
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .take(8)
            .collect::<String>()
    );

    let mut answer = fields.clone();
    answer["id"] = saved.get("id").cloned().unwrap_or(Value::Null);

    Ok(json!({
        "utilization": answer,
        "journal_entry": {
            "date": utilization_date,
            "description": label,
            "entries": [
                line("3320", "Revaluation Reserve", &amount_moved, &Money::zero()),
                line(credit_account.0, credit_account.1, &Money::zero(), &amount_moved),
            ],
            "reference": reference,
        },
    }))
}

fn cumulative_for<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    asset_id: &str,
) -> Result<Option<Value>, StoreError> {
    let rows = Records::new(graph, CUMULATIVE_REVALUATION).list(scope, 1000)?;
    Ok(rows
        .into_iter()
        .find(|row| row.get("asset_id").and_then(Value::as_str) == Some(asset_id)))
}

/// `/revaluations`: the caller's entries, optionally narrowed.
pub fn list_revaluations<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
    asset_id: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, REVALUATION_ENTRY).list(scope, 1000)?;
    let keep = |row: &Value, key: &str, wanted: Option<&str>| match wanted {
        None => true,
        Some(value) => row.get(key).and_then(Value::as_str) == Some(value),
    };
    let filtered: Vec<Value> = rows
        .into_iter()
        .filter(|row| keep(row, "company_id", company_id))
        .filter(|row| keep(row, "asset_id", asset_id))
        .collect();
    Ok(json!({"revaluations": filtered}))
}

/// `/cumulative/{asset_id}`: the running reserve, or the service's own miss shape.
pub fn cumulative<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    asset_id: &str,
) -> Result<Value, StoreError> {
    Ok(match cumulative_for(graph, scope, asset_id)? {
        Some(record) => record,
        None => json!({"error": "Asset not found"}),
    })
}

/// `/summary/{company_id}`: gains, losses, utilizations and what is left of the reserve.
pub fn summary<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let entries = Records::new(graph, REVALUATION_ENTRY).list(scope, 1000)?;
    let mut gains = Money::zero();
    let mut losses = Money::zero();
    let mut assets: Vec<String> = Vec::new();
    for row in entries
        .iter()
        .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
    {
        gains = gains.plus(&stored_amount(row, "revaluation_gain")?);
        losses = losses.plus(&stored_amount(row, "revaluation_loss")?);
        if let Some(asset) = row.get("asset_id").and_then(Value::as_str) {
            if !assets.iter().any(|seen| seen == asset) {
                assets.push(asset.to_string());
            }
        }
    }

    let utilizations = Records::new(graph, REVALUATION_UTILIZATION).list(scope, 1000)?;
    let mut utilized = Money::zero();
    for row in utilizations
        .iter()
        .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
    {
        utilized = utilized.plus(&stored_amount(row, "amount")?);
    }

    Ok(json!({
        "company_id": company_id,
        "total_revaluation_gains": gains.round(2).to_fixed(2),
        "total_revaluation_losses": losses.round(2).to_fixed(2),
        "total_utilized": utilized.round(2).to_fixed(2),
        "net_revaluation_reserve": gains.minus(&losses).minus(&utilized).round(2).to_fixed(2),
        "assets_revalued": assets.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn payload(previous: &str, new_value: &str) -> Value {
        json!({
            "company_id": "co-1",
            "asset_id": "a-1",
            "asset_name": "Head office",
            "asset_class": "property",
            "revaluation_date": "2026-10-31",
            "previous_value": previous,
            "new_value": new_value,
            "depreciation_adjustment": "0"
        })
    }

    #[test]
    fn an_upward_revaluation_is_a_gain_against_the_reserve() {
        let graph = FakeGraph::returning(vec![json!({"id": "r-1", "asset_id": "a-1"})]);
        let answer =
            record_revaluation(&graph, &scope(), &payload("100000", "150000")).expect("recorded");
        assert_eq!(
            answer["revaluation"]["revaluation_gain"],
            json!("50000.000000")
        );
        assert_eq!(answer["revaluation"]["revaluation_loss"], json!("0.000000"));
        // the property account is debited, the reserve credited
        assert_eq!(
            answer["journal_entry"]["entries"][0]["account_code"],
            json!("1500")
        );
        assert_eq!(
            answer["journal_entry"]["entries"][0]["debit"],
            json!("50000.00")
        );
        assert_eq!(
            answer["journal_entry"]["entries"][1]["account_code"],
            json!("3320")
        );
        assert_eq!(
            answer["journal_entry"]["entries"][1]["credit"],
            json!("50000.00")
        );
    }

    #[test]
    fn a_downward_revaluation_credits_the_asset_and_debits_the_reserve() {
        let graph = FakeGraph::returning(vec![json!({"id": "r-2", "asset_id": "a-1"})]);
        let answer =
            record_revaluation(&graph, &scope(), &payload("150000", "120000")).expect("recorded");
        assert_eq!(
            answer["revaluation"]["revaluation_loss"],
            json!("30000.000000")
        );
        assert_eq!(
            answer["journal_entry"]["entries"][0]["account_code"],
            json!("3320")
        );
        assert_eq!(
            answer["journal_entry"]["entries"][0]["debit"],
            json!("30000.00")
        );
        assert_eq!(
            answer["journal_entry"]["entries"][1]["credit"],
            json!("30000.00")
        );
        assert!(answer["journal_entry"]["description"]
            .as_str()
            .unwrap()
            .contains("downward"));
    }

    #[test]
    fn an_unchanged_valuation_records_a_zero_loss() {
        let graph = FakeGraph::returning(vec![json!({"id": "r-3"})]);
        let answer =
            record_revaluation(&graph, &scope(), &payload("500", "500")).expect("recorded");
        assert_eq!(answer["revaluation"]["revaluation_gain"], json!("0.000000"));
        assert_eq!(answer["revaluation"]["revaluation_loss"], json!("0.000000"));
    }

    #[test]
    fn the_net_effect_can_be_negative_once_the_adjustment_is_taken_off() {
        let graph = FakeGraph::returning(vec![json!({"id": "r-4"})]);
        let mut body = payload("1000", "1500");
        body["depreciation_adjustment"] = json!("700");
        let answer = record_revaluation(&graph, &scope(), &body).expect("recorded");
        // 500 gain less the 700 adjustment
        assert_eq!(answer["revaluation"]["net_effect"], json!("-200.000000"));
    }

    #[test]
    fn a_plant_revaluation_uses_the_1600_account() {
        let graph = FakeGraph::returning(vec![json!({"id": "r-5"})]);
        let mut body = payload("1000", "2000");
        body["asset_class"] = json!("plant");
        let answer = record_revaluation(&graph, &scope(), &body).expect("recorded");
        assert_eq!(
            answer["journal_entry"]["entries"][0]["account_code"],
            json!("1600")
        );
    }

    #[test]
    fn a_first_revaluation_starts_the_reserve_at_the_difference() {
        let graph = FakeGraph::returning(vec![json!({"id": "r-6", "asset_id": "a-1"})]);
        let answer =
            record_revaluation(&graph, &scope(), &payload("200000", "260000")).expect("recorded");
        assert_eq!(
            answer["cumulative"]["total_revaluation_gain"],
            json!("60000.000000")
        );
        assert_eq!(
            answer["cumulative"]["net_revaluation_reserve"],
            json!("60000.000000")
        );
        assert_eq!(answer["cumulative"]["total_utilized"], json!("0.000000"));
        assert_eq!(answer["cumulative"]["asset_id"], json!("a-1"));
    }

    #[test]
    fn a_later_revaluation_adds_to_the_running_reserve() {
        let graph = FakeGraph::returning(vec![json!({"id": "r-7", "asset_id": "a-1"})]);
        // the cumulative read finds the existing record, with a utilization already against it
        let answer = {
            let reading = FakeGraph::returning(vec![json!({
                "id": "c-1", "asset_id": "a-1", "company_id": "co-1",
                "total_revaluation_gain": "60000", "total_revaluation_loss": "0",
                "total_utilized": "10000", "net_revaluation_reserve": "50000"
            })]);
            record_revaluation(&reading, &scope(), &payload("260000", "250000")).expect("recorded")
        };
        drop(graph);
        assert_eq!(
            answer["cumulative"]["total_revaluation_gain"],
            json!("60000.000000")
        );
        // a 10000 fall is added to the losses and taken off, leaving 40000 after the utilization
        assert_eq!(
            answer["cumulative"]["total_revaluation_loss"],
            json!("10000.000000")
        );
        assert_eq!(
            answer["cumulative"]["net_revaluation_reserve"],
            json!("40000.000000")
        );
    }

    #[test]
    fn a_disposal_and_a_transfer_credit_the_same_account() {
        for utilization_type in ["asset_disposal", "transfer_retained_earnings"] {
            let graph = FakeGraph::returning(vec![json!({"id": "u-1", "company_id": "co-1"})]);
            let answer = record_utilization(
                &graph,
                &scope(),
                &json!({"company_id": "co-1", "amount": "5000",
                        "utilization_type": utilization_type, "description": "part sold"}),
            )
            .expect("recorded");
            assert_eq!(
                answer["journal_entry"]["entries"][0]["account_code"],
                json!("3320")
            );
            assert_eq!(
                answer["journal_entry"]["entries"][0]["debit"],
                json!("5000.00")
            );
            assert_eq!(
                answer["journal_entry"]["entries"][1]["account_code"],
                json!("3300")
            );
            assert_eq!(
                answer["journal_entry"]["entries"][1]["credit"],
                json!("5000.00")
            );
        }
    }

    #[test]
    fn an_impairment_credits_the_impairment_account() {
        let graph = FakeGraph::returning(vec![json!({"id": "u-2", "company_id": "co-1"})]);
        let answer = record_utilization(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "amount": "1200",
                    "utilization_type": "impairment", "description": "site damage"}),
        )
        .expect("recorded");
        assert_eq!(
            answer["journal_entry"]["entries"][1]["account_code"],
            json!("1650")
        );
        assert_eq!(
            answer["journal_entry"]["entries"][1]["credit"],
            json!("1200.00")
        );
    }

    #[test]
    fn the_summary_uses_gains_losses_and_utilizations_together() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "r-1", "company_id": "co-1", "asset_id": "a-1",
                   "revaluation_gain": "50000", "revaluation_loss": "0"}),
            json!({"id": "r-2", "company_id": "co-1", "asset_id": "a-2",
                   "revaluation_gain": "0", "revaluation_loss": "20000"}),
            json!({"id": "r-3", "company_id": "co-2", "asset_id": "a-9",
                   "revaluation_gain": "999999", "revaluation_loss": "0"}),
        ]);
        let answer = summary(&graph, &scope(), "co-1").expect("summary");
        assert_eq!(answer["total_revaluation_gains"], json!("50000.00"));
        assert_eq!(answer["total_revaluation_losses"], json!("20000.00"));
        assert_eq!(answer["assets_revalued"], json!(2));
    }

    #[test]
    fn a_cumulative_miss_answers_the_services_own_shape() {
        let graph = FakeGraph::returning(vec![]);
        let answer = cumulative(&graph, &scope(), "nope").expect("answer");
        assert_eq!(answer, json!({"error": "Asset not found"}));
    }
}
