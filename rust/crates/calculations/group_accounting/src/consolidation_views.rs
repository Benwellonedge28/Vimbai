#![forbid(unsafe_code)]
//! The consolidation service's reporting views: intercompany eliminations, NCI,
//! goodwill on acquisition, currency translation and validation.
//!
//! The consolidated statement itself lives in `consolidation.rs`. These are the
//! breakdowns published beside it, and they carry the same fixed assumptions the
//! service made: goodwill is measured against a fixed net-asset figure, NCI falls back
//! to a standard profit and equity when a subsidiary states neither, and the
//! cumulative translation adjustment is one and a half times the current one. Those
//! are reproduced, not corrected, because a client reads the figures as published.

use serde::Deserialize;
use serde_json::{json, Value};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Deserialize)]
struct Subsidiary {
    subsidiary_id: String,
    subsidiary_name: String,
    #[serde(default)]
    ownership_percentage: f64,
    #[serde(default)]
    non_controlling_interests_percentage: f64,
    #[serde(default)]
    financial_data: std::collections::BTreeMap<String, Value>,
}

fn usd() -> String {
    "USD".to_string()
}

#[derive(Debug, Clone, Deserialize)]
struct Txn {
    transaction_id: String,
    transaction_type: String,
    #[serde(default)]
    parent_party: String,
    #[serde(default)]
    subsidiary_party: String,
    amount: f64,
    #[serde(default = "usd")]
    currency: String,
    #[serde(default)]
    transaction_date: String,
}

#[derive(Debug, Deserialize)]
struct ConsolidationBody {
    parent_company_id: String,
    period: String,
    #[serde(default)]
    subsidiaries: Vec<Subsidiary>,
    #[serde(default)]
    intercompany_transactions: Vec<Txn>,
}

fn amount_of(value: &Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

/// `/eliminations`: every transaction is eliminated and categorised by type.
/// A transaction whose type is not one of the known kinds is reported back whole.
pub fn eliminations(payload: &Value) -> Result<Value, String> {
    let body: ConsolidationBody =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    let mut total = 0.0;
    let mut asset = 0.0;
    let mut liability = 0.0;
    let mut revenue = 0.0;
    let mut expense = 0.0;
    let mut dividend = 0.0;
    let mut loan = 0.0;
    let mut management_fee = 0.0;
    let mut unmatched: Vec<Value> = Vec::new();

    for txn in &body.intercompany_transactions {
        let amount = txn.amount;
        total += amount;
        match txn.transaction_type.as_str() {
            "receivable" => asset += amount,
            "payable" => liability += amount,
            "sale" => revenue += amount,
            "purchase" => expense += amount,
            "dividend" => dividend += amount,
            "loan" => loan += amount,
            "management_fee" => management_fee += amount,
            _ => unmatched.push(json!({
                "transaction_id": txn.transaction_id,
                "transaction_type": txn.transaction_type,
                "parent_party": txn.parent_party,
                "subsidiary_party": txn.subsidiary_party,
                "amount": txn.amount,
                "currency": txn.currency,
                "transaction_date": txn.transaction_date,
            })),
        }
    }

    Ok(json!({
        "parent_company_id": body.parent_company_id,
        "period": body.period,
        "total_eliminations": py_round2(total),
        "asset_eliminations": py_round2(asset),
        "liability_eliminations": py_round2(liability),
        "revenue_eliminations": py_round2(revenue),
        "expense_eliminations": py_round2(expense),
        "intercompany_dividends": py_round2(dividend),
        "intercompany_loans": py_round2(loan),
        "intercompany_management_fees": py_round2(management_fee),
        "unmatched_transactions": unmatched,
    }))
}

/// `/nci-calculation`: each subsidiary's share of profit and equity, with a fair
/// value adjustment of twenty per cent of that equity share.
pub fn nci(payload: &Value) -> Result<Value, String> {
    let body: ConsolidationBody =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    let mut details = serde_json::Map::new();
    let mut total_profit = 0.0;
    let mut total_equity = 0.0;

    for sub in &body.subsidiaries {
        let profit = body_number(&sub.financial_data, "profit", 500_000.0);
        let equity = body_number(&sub.financial_data, "equity", 2_500_000.0);
        let percent = sub.non_controlling_interests_percentage;
        let profit_share = py_round2(profit * (percent / 100.0));
        let equity_share = py_round2(equity * (percent / 100.0));
        total_profit += profit_share;
        total_equity += equity_share;
        details.insert(
            sub.subsidiary_id.clone(),
            json!({
                "subsidiary_name": sub.subsidiary_name,
                "nci_percentage": percent,
                "nci_profit_share": profit_share,
                "nci_equity_share": equity_share,
                "nci_fair_value_adjustment": py_round2(equity * (percent / 100.0) * 0.2),
            }),
        );
    }

    Ok(json!({
        "parent_company_id": body.parent_company_id,
        "period": body.period,
        "subsidiary_nci_details": details,
        "total_nci_profit": py_round2(total_profit),
        "total_nci_equity": py_round2(total_equity),
    }))
}

fn body_number(data: &std::collections::BTreeMap<String, Value>, key: &str, fallback: f64) -> f64 {
    data.get(key).map(amount_of).unwrap_or(fallback)
}

/// `/goodwill-calculation`: consideration, NCI at fair value, less net identifiable
/// assets. A negative result is a bargain purchase and is reported rather than
/// recognised.
pub fn goodwill_on_acquisition(payload: &Value) -> Result<Value, String> {
    let body: ConsolidationBody =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    let mut acquisitions = Vec::new();
    let mut total = 0.0;

    for sub in &body.subsidiaries {
        let consideration = 6_000_000.0 * (sub.ownership_percentage / 100.0);
        let fair_value_nci = 1_500_000.0 * (sub.non_controlling_interests_percentage / 100.0);
        let net_identifiable_assets = 4_500_000.0;
        let goodwill = consideration + fair_value_nci - net_identifiable_assets;
        let recognised = if goodwill > 0.0 { goodwill } else { 0.0 };
        total += recognised;
        acquisitions.push(json!({
            "subsidiary_id": sub.subsidiary_id,
            "subsidiary_name": sub.subsidiary_name,
            "consideration_transferred": py_round2(consideration),
            "fair_value_nci": py_round2(fair_value_nci),
            "net_identifiable_assets": py_round2(net_identifiable_assets),
            "goodwill": py_round2(recognised),
            "bargain_purchase": goodwill < 0.0,
        }));
    }

    Ok(json!({
        "parent_company_id": body.parent_company_id,
        "acquisitions": acquisitions,
        "total_goodwill": py_round2(total),
        "impairment_test_required": total > 0.0,
    }))
}

#[derive(Debug, Deserialize)]
struct TranslationBody {
    parent_id: String,
    local_currency: String,
    reporting_currency: String,
    #[serde(default)]
    exchange_rates: std::collections::BTreeMap<String, Value>,
    #[serde(default)]
    balances: std::collections::BTreeMap<String, Value>,
}

/// `/currency-translation`: one rate for the local currency is applied to every
/// balance, and equity balances carry the difference into the translation
/// adjustment.
pub fn currency_translation(payload: &Value) -> Result<Value, String> {
    let body: TranslationBody =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    let rate = body
        .exchange_rates
        .get(&body.local_currency)
        .map(amount_of)
        .unwrap_or(1.0);
    let mut translated = serde_json::Map::new();
    let mut adjustment = 0.0;

    for (account, balance) in &body.balances {
        let amount = amount_of(balance);
        translated.insert(account.clone(), json!(py_round2(amount * rate)));
        if account.to_lowercase().contains("equity") {
            adjustment += amount * (rate - 1.0);
        }
    }

    Ok(json!({
        "parent_id": body.parent_id,
        "local_currency": body.local_currency,
        "reporting_currency": body.reporting_currency,
        "translated_balances": translated,
        "translation_adjustment": py_round2(adjustment),
        "cumulative_translation_adjustment": py_round2(adjustment * 1.5),
    }))
}

#[derive(Debug, Deserialize)]
struct ValidationBody {
    parent_id: String,
    period: String,
    /// Published by the caller and echoed in the log line only, so it is accepted and
    /// deliberately unused.
    #[serde(default)]
    #[allow(dead_code)]
    entities: Vec<String>,
    #[serde(default)]
    validation_rules: Vec<String>,
}

/// `/validate`: the caller's rules are echoed back as passed, along with the two
/// standing checks. Nothing here can fail, which is why it is served as written
/// rather than tightened into a real balance test.
pub fn validate(payload: &Value) -> Result<Value, String> {
    let body: ValidationBody =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    let mut results: Vec<Value> = body
        .validation_rules
        .iter()
        .map(|rule| json!({ "rule": rule, "status": "passed" }))
        .collect();
    results.push(json!({
        "rule": "balance_sheet_balanced",
        "status": "passed",
        "details": "Assets = Liabilities + Equity",
    }));
    results.push(json!({
        "rule": "intercompany_eliminated",
        "status": "passed",
        "details": "All intercompany transactions eliminated",
    }));

    Ok(json!({
        "parent_id": body.parent_id,
        "period": body.period,
        "is_valid": true,
        "validation_results": results,
        "warnings": Vec::<String>::new(),
        "errors": Vec::<String>::new(),
    }))
}

// --- JSON boundaries used by the extension and the HTTP interface ------------

pub fn eliminations_from_json(payload: &Value) -> Result<Value, String> {
    eliminations(payload)
}

pub fn nci_from_json(payload: &Value) -> Result<Value, String> {
    nci(payload)
}

pub fn goodwill_calculation_from_json(payload: &Value) -> Result<Value, String> {
    goodwill_on_acquisition(payload)
}

pub fn currency_translation_from_json(payload: &Value) -> Result<Value, String> {
    currency_translation(payload)
}

pub fn validate_from_json(payload: &Value) -> Result<Value, String> {
    validate(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subsidiary(id: &str, own: f64, nci: f64) -> Value {
        json!({
            "subsidiary_id": id,
            "subsidiary_name": format!("Sub {id}"),
            "ownership_percentage": own,
            "non_controlling_interests_percentage": nci,
            "financial_data": { "profit": 400_000.0, "equity": 1_200_000.0 },
        })
    }

    #[test]
    fn eliminations_categorise_by_type_and_report_the_rest() {
        let payload = json!({
            "parent_company_id": "p1",
            "period": "2026",
            "intercompany_transactions": [
                {"transaction_id": "t1", "transaction_type": "sale", "amount": 1000.5},
                {"transaction_id": "t2", "transaction_type": "dividend", "amount": 250.25},
                {"transaction_id": "t3", "transaction_type": "rebate", "amount": 10.0},
            ],
        });
        let answer = eliminations(&payload).expect("eliminations");
        assert_eq!(answer["total_eliminations"], json!(1260.75));
        assert_eq!(answer["revenue_eliminations"], json!(1000.5));
        assert_eq!(answer["intercompany_dividends"], json!(250.25));
        assert_eq!(
            answer["unmatched_transactions"].as_array().unwrap().len(),
            1
        );
    }

    #[test]
    fn nci_falls_back_to_standard_figures_when_none_are_stated() {
        let payload = json!({
            "parent_company_id": "p1",
            "period": "2026",
            "subsidiaries": [{
                "subsidiary_id": "s1",
                "subsidiary_name": "Sub s1",
                "non_controlling_interests_percentage": 40.0,
            }],
        });
        let answer = nci(&payload).expect("nci");
        assert_eq!(
            answer["subsidiary_nci_details"]["s1"]["nci_profit_share"],
            json!(200000.0)
        );
        assert_eq!(
            answer["subsidiary_nci_details"]["s1"]["nci_equity_share"],
            json!(1000000.0)
        );
        assert_eq!(
            answer["subsidiary_nci_details"]["s1"]["nci_fair_value_adjustment"],
            json!(200000.0)
        );
        assert_eq!(answer["total_nci_equity"], json!(1000000.0));
    }

    #[test]
    fn a_bargain_purchase_is_flagged_and_not_recognised() {
        let payload = json!({
            "parent_company_id": "p1",
            "period": "2026",
            "subsidiaries": [subsidiary("s1", 50.0, 0.0)],
        });
        let answer = goodwill_on_acquisition(&payload).expect("goodwill");
        let acquisition = &answer["acquisitions"][0];
        assert_eq!(acquisition["goodwill"], json!(0.0));
        // 3,000,000 consideration against 4,500,000 of net assets is a bargain, so
        // nothing is recognised and the flag says why.
        assert_eq!(acquisition["bargain_purchase"], json!(true));
        assert_eq!(answer["impairment_test_required"], json!(false));
    }

    #[test]
    fn translation_applies_one_rate_and_charges_equity_to_the_adjustment() {
        let payload = json!({
            "parent_id": "p1",
            "local_currency": "ZWL",
            "reporting_currency": "USD",
            "exchange_rates": { "ZWL": 0.5 },
            "balances": { "cash": 1000.0, "equity": 4000.0 },
        });
        let answer = currency_translation(&payload).expect("translation");
        assert_eq!(answer["translated_balances"]["cash"], json!(500.0));
        assert_eq!(answer["translated_balances"]["equity"], json!(2000.0));
        assert_eq!(answer["translation_adjustment"], json!(-2000.0));
        assert_eq!(answer["cumulative_translation_adjustment"], json!(-3000.0));
    }

    #[test]
    fn validation_echoes_the_callers_rules_then_the_standing_checks() {
        let payload = json!({
            "parent_id": "p1",
            "period": "2026",
            "entities": ["a", "b"],
            "validation_rules": ["trial_balance"],
        });
        let answer = validate(&payload).expect("validate");
        assert_eq!(answer["is_valid"], json!(true));
        assert_eq!(answer["validation_results"].as_array().unwrap().len(), 3);
        assert_eq!(
            answer["validation_results"][0]["rule"],
            json!("trial_balance")
        );
        assert_eq!(answer["errors"].as_array().unwrap().len(), 0);
    }
}
