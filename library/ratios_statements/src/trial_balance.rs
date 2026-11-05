//! Trial balance generation, validation and the adjusted book.
//!
//! A few behaviours are the service's and are kept:
//!
//! `account_type` decides which way the balance is measured. Assets and expenses are debit-natural,
//! so their balance is debit minus credit; everything else is credit-natural, measured as credit
//! minus debit. An account with an unrecognised type, or none at all - which is what an adjustment
//! creates when it adds a new account - is treated as credit-natural.
//!
//! That measured balance is then placed in the debit column whenever it is positive. This is the
//! most serious defect found in any ported calculator and it is preserved rather than corrected:
//! a credit-natural account with a normal credit balance therefore lands in the *debit* column. A
//! plain two-line book - cash 500 debit, payables 500 credit - reports 1000 of debits, nothing of
//! credits, and a status of "Disagree - Check balances". Every credit-natural account with a
//! positive balance is misclassified, so the endpoint can only agree when those accounts are
//! overdrawn. Anything reading `status` from this service has been getting "Disagree" for correct
//! books.
//!
//! `classification_summary` adds the debit and credit columns together instead of netting them. A
//! liability with a 400 credit and a 100 debit reports 500, and the section total is a sum of both
//! columns rather than a true balance. It reads as a balance and is not one.
//!
//! `adjusted` applies each adjustment to the first matching account only, so an adjustment against a
//! duplicated account name leaves the second copy untouched.

use serde_json::{Map, Value};

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

fn number(row: &Value, key: &str) -> f64 {
    row.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn text(row: &Value, key: &str, fallback: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

/// `POST /generate` and `POST /from-ledgers`.
pub fn generate(payload: &Value) -> Result<Value, String> {
    let accounts = match payload {
        Value::Array(rows) => rows.clone(),
        other => other
            .get("accounts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };

    let mut rows = Vec::with_capacity(accounts.len());
    let mut total_debits = 0.0;
    let mut total_credits = 0.0;

    for account in &accounts {
        let kind = text(account, "account_type", "");
        let debit = number(account, "debit_balance");
        let credit = number(account, "credit_balance");
        let balance = if kind == "asset" || kind == "expense" {
            debit - credit
        } else {
            credit - debit
        };
        if balance >= 0.0 {
            let dr = if balance > 0.0 { balance } else { 0.0 };
            total_debits += dr;
            rows.push(serde_json::json!({
                "account_name": text(account, "account_name", "Unknown"),
                "account_type": kind,
                "debit": dr,
                "credit": 0.0,
            }));
        } else {
            total_credits += balance.abs();
            rows.push(serde_json::json!({
                "account_name": text(account, "account_name", "Unknown"),
                "account_type": kind,
                "debit": 0.0,
                "credit": balance.abs(),
            }));
        }
    }

    let balanced = (total_debits - total_credits).abs() < 0.01;
    Ok(serde_json::json!({
        "trial_balance": rows,
        "total_debits": round2(total_debits),
        "total_credits": round2(total_credits),
        "is_balanced": balanced,
        "difference": round2(total_debits - total_credits),
        "status": if balanced { "Agree" } else { "Disagree - Check balances" },
    }))
}

/// `POST /validate`. Takes the two totals as query parameters.
pub fn validate(payload: &Value) -> Result<Value, String> {
    let debits = number(payload, "total_debits");
    let credits = number(payload, "total_credits");
    let difference = (debits - credits).abs();
    let valid = difference < 0.01;
    Ok(serde_json::json!({
        "total_debits": debits,
        "total_credits": credits,
        "difference": round2(difference),
        "is_valid": valid,
        "message": if valid {
            "Trial balance agrees".to_string()
        } else {
            format!("Trial balance disagrees by {difference}")
        },
    }))
}

/// `POST /classification-summary`.
pub fn classification_summary(payload: &Value) -> Result<Value, String> {
    let accounts = match payload {
        Value::Array(rows) => rows.clone(),
        other => other
            .get("accounts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };

    let mut sections: Vec<(&str, Vec<&Value>)> = vec![
        ("assets", Vec::new()),
        ("liabilities", Vec::new()),
        ("equity", Vec::new()),
        ("revenues", Vec::new()),
        ("expenses", Vec::new()),
    ];
    for account in &accounts {
        let kind = text(account, "account_type", "");
        let slot = match kind.as_str() {
            "asset" => Some(0),
            "liability" => Some(1),
            "equity" => Some(2),
            "revenue" => Some(3),
            "expense" => Some(4),
            _ => None,
        };
        if let Some(index) = slot {
            sections[index].1.push(account);
        }
    }

    let mut out = Map::new();
    for (name, rows) in sections {
        let total: f64 = rows
            .iter()
            .map(|row| number(row, "debit_balance") + number(row, "credit_balance"))
            .sum();
        out.insert(
            name.to_string(),
            serde_json::json!({
                "accounts": rows.into_iter().cloned().collect::<Vec<Value>>(),
                "total": total,
            }),
        );
    }
    Ok(Value::Object(out))
}

/// `POST /adjusted`.
pub fn adjusted(payload: &Value) -> Result<Value, String> {
    let mut book = payload
        .get("pre_adjustment")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let adjustments = payload
        .get("adjustments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for adjustment in &adjustments {
        let name = adjustment
            .get("account_name")
            .cloned()
            .unwrap_or(Value::Null);
        let debit = number(adjustment, "debit");
        let credit = number(adjustment, "credit");
        let mut applied = false;
        for account in book.iter_mut() {
            if account.get("account_name") == Some(&name) {
                if let Some(object) = account.as_object_mut() {
                    let new_debit = number(&Value::Object(object.clone()), "debit_balance") + debit;
                    let new_credit =
                        number(&Value::Object(object.clone()), "credit_balance") + credit;
                    object.insert("debit_balance".into(), Value::from(new_debit));
                    object.insert("credit_balance".into(), Value::from(new_credit));
                }
                applied = true;
                break;
            }
        }
        if !applied {
            book.push(serde_json::json!({
                "account_name": name,
                "debit_balance": debit,
                "credit_balance": credit,
            }));
        }
    }

    generate(&Value::Array(book))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn accounts() -> Value {
        json!([
            {"account_name": "Cash", "account_type": "asset", "debit_balance": 500.0},
            {"account_name": "Payables", "account_type": "liability", "credit_balance": 500.0}
        ])
    }

    #[test]
    fn a_correct_book_reports_everything_as_debit() {
        // The defect, pinned: cash 500 debit and payables 500 credit is a balanced book, and the
        // service calls it 1000 of debits, no credits, and a disagreement.
        let out = generate(&accounts()).unwrap();
        assert_eq!(out["total_debits"], json!(1000.0));
        assert_eq!(out["total_credits"], json!(0.0));
        assert_eq!(out["is_balanced"], json!(false));
        assert_eq!(out["status"], json!("Disagree - Check balances"));
    }

    #[test]
    fn a_credit_balance_lands_in_the_debit_column() {
        let out = generate(&json!([
            {"account_name": "Cash", "account_type": "asset",
             "debit_balance": 900.0, "credit_balance": 100.0},
            {"account_name": "Loan", "account_type": "liability",
             "debit_balance": 100.0, "credit_balance": 900.0}
        ]))
        .unwrap();
        assert_eq!(out["trial_balance"][0]["debit"], json!(800.0));
        assert_eq!(out["trial_balance"][1]["debit"], json!(800.0));
        assert_eq!(out["trial_balance"][1]["credit"], json!(0.0));
    }

    #[test]
    fn only_an_overdrawn_credit_account_reaches_the_credit_column() {
        let out = generate(&json!([
            {"account_name": "Loan", "account_type": "liability",
             "debit_balance": 600.0, "credit_balance": 200.0}
        ]))
        .unwrap();
        assert_eq!(out["trial_balance"][0]["credit"], json!(400.0));
        assert_eq!(out["total_credits"], json!(400.0));
    }

    #[test]
    fn an_unknown_type_falls_back_to_credit_natural() {
        let out = generate(&json!([
            {"account_name": "Suspense", "debit_balance": 500.0}
        ]))
        .unwrap();
        assert_eq!(out["trial_balance"][0]["credit"], json!(500.0));
        assert_eq!(out["status"], json!("Disagree - Check balances"));
    }

    #[test]
    fn validate_reports_the_gap_it_found() {
        let out = validate(&json!({"total_debits": 1000.0, "total_credits": 995.5})).unwrap();
        assert_eq!(out["difference"], json!(4.5));
        assert_eq!(out["is_valid"], json!(false));
        assert_eq!(out["message"], json!("Trial balance disagrees by 4.5"));
    }

    #[test]
    fn the_classification_summary_adds_both_columns_together() {
        let out = classification_summary(&json!([
            {"account_name": "Loan", "account_type": "liability",
             "debit_balance": 100.0, "credit_balance": 400.0}
        ]))
        .unwrap();
        // a 300 credit balance is reported as 500
        assert_eq!(out["liabilities"]["total"], json!(500.0));
    }

    #[test]
    fn an_adjustment_lands_on_the_first_matching_account() {
        let out = adjusted(&json!({
            "pre_adjustment": [
                {"account_name": "Rent", "account_type": "expense", "debit_balance": 100.0},
                {"account_name": "Rent", "account_type": "expense", "debit_balance": 100.0},
                {"account_name": "Accruals", "account_type": "liability", "credit_balance": 0.0}
            ],
            "adjustments": [
                {"account_name": "Rent", "debit": 50.0, "credit": 0.0},
                {"account_name": "Accruals", "debit": 0.0, "credit": 50.0}
            ]
        }))
        .unwrap();
        assert_eq!(out["trial_balance"][0]["debit"], json!(150.0));
        assert_eq!(out["trial_balance"][1]["debit"], json!(100.0));
        // debits 300 against no credits, for the reason given above
        assert_eq!(out["is_balanced"], json!(false));
    }

    #[test]
    fn a_new_account_from_an_adjustment_has_no_type_so_lands_in_credit() {
        let out = adjusted(&json!({
            "pre_adjustment": [],
            "adjustments": [{"account_name": "New expense", "debit": 75.0}]
        }))
        .unwrap();
        assert_eq!(out["trial_balance"][0]["account_type"], json!(""));
        assert_eq!(out["trial_balance"][0]["credit"], json!(75.0));
    }
}
