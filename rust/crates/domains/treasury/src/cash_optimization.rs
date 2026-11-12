//! cash-optimization-service's accounts and suggestions, on the store.
//!
//! The rules are the ones the service published, kept as they were: excess operating cash above
//! its minimum moves to the best-yielding investment account, excess reserve cash only moves when
//! another account pays more, and an underfunded reserve, tax or payroll account is topped up from
//! the operating account with the largest cushion. Thresholds, priorities and the wording of the
//! reasons are unchanged, and each run replaces the previous set of suggestions for the company.
//!
//! The arithmetic runs on `Money`, so a seven-figure balance does not shift in the last digits
//! while it is being compared and multiplied by a rate.

#![forbid(unsafe_code)]

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{stored_amount, wire};
use vimbai_store::{BookScope, Graph, Records, StoreError, CASH_ACCOUNT, CASH_SUGGESTION};

/// One account, read from the graph.
struct Account {
    name: String,
    kind: String,
    balance: Money,
    minimum: Money,
    rate: Money,
}

impl Account {
    fn excess(&self) -> Money {
        self.balance.minus(&self.minimum)
    }
}

fn accounts_of(rows: &[Value]) -> Result<Vec<Account>, StoreError> {
    let mut out = Vec::new();
    for row in rows {
        out.push(Account {
            name: row
                .get("account_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            kind: row
                .get("account_type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            balance: stored_amount(row, "balance")?,
            minimum: stored_amount(row, "min_required")?,
            rate: stored_amount(row, "interest_rate")?,
        });
    }
    Ok(out)
}

fn account_json(row: &Value) -> Result<Value, StoreError> {
    Ok(json!({
        "id": row.get("id").cloned().unwrap_or(Value::Null),
        "company_id": row.get("company_id").cloned().unwrap_or(Value::Null),
        "account_name": row.get("account_name").cloned().unwrap_or(Value::Null),
        "account_type": row.get("account_type").cloned().unwrap_or(Value::Null),
        "balance": wire(&stored_amount(row, "balance")?),
        "min_required": wire(&stored_amount(row, "min_required")?),
        "interest_rate": wire(&stored_amount(row, "interest_rate")?),
        "currency": row.get("currency").cloned().unwrap_or(Value::Null),
        "created_at": row.get("created_at").cloned().unwrap_or(Value::Null),
    }))
}

/// The suggestion as the service published it: one field per line item of its model.
#[allow(clippy::too_many_arguments)]
fn suggestion_json(
    row: &Value,
    company_id: &str,
    from: &str,
    to: &str,
    amount: &Money,
    benefit: &Money,
    reason: &str,
    priority: &str,
) -> Value {
    json!({
        "id": row.get("id").cloned().unwrap_or(Value::Null),
        "company_id": company_id,
        "from_account": from,
        "to_account": to,
        "amount": wire(amount),
        "reason": reason,
        "expected_benefit": wire(benefit),
        "priority": priority,
        "created_at": row.get("created_at").cloned().unwrap_or(Value::Null),
    })
}

/// `/accounts`: add an account.
pub fn add_account<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = payload
        .get("company_id")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("company_id is required".into()))?;
    let account_name = payload
        .get("account_name")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("account_name is required".into()))?;
    let account_type = payload
        .get("account_type")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("account_type is required".into()))?;
    let balance = match payload.get("balance") {
        Some(Value::String(text)) => {
            Money::parse(text).map_err(|error| StoreError::Invalid(format!("balance: {error}")))?
        }
        Some(Value::Number(number)) => Money::from_f64_shortest(number.as_f64().unwrap_or(0.0))
            .map_err(|error| StoreError::Invalid(format!("balance: {error}")))?,
        _ => return Err(StoreError::Invalid("balance is required".into())),
    };
    let minimum = match payload.get("min_required") {
        Some(value) => Money::parse(&value.to_string())
            .or_else(|_| Money::from_f64_shortest(value.as_f64().unwrap_or(0.0)))
            .map_err(|error| StoreError::Invalid(format!("min_required: {error}")))?,
        None => Money::zero(),
    };
    let rate = match payload.get("interest_rate") {
        Some(value) => Money::parse(&value.to_string())
            .or_else(|_| Money::from_f64_shortest(value.as_f64().unwrap_or(0.0)))
            .map_err(|error| StoreError::Invalid(format!("interest_rate: {error}")))?,
        None => Money::zero(),
    };

    let stored = Records::new(graph, CASH_ACCOUNT).create(
        scope,
        &json!({
            "company_id": company_id,
            "account_name": account_name,
            "account_type": account_type,
            "balance": balance.to_fixed(6),
            "min_required": minimum.to_fixed(6),
            "interest_rate": rate.to_fixed(6),
            "currency": payload.get("currency").and_then(Value::as_str).unwrap_or("USD"),
            "created_at": payload.get("created_at").and_then(Value::as_str).unwrap_or(""),
        }),
    )?;

    // The service answered with just these three fields.
    Ok(json!({
        "id": stored.get("id").cloned().unwrap_or(Value::Null),
        "account_name": account_name,
        "balance": wire(&balance),
    }))
}

/// The caller's accounts for one company.
fn company_accounts<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Vec<Value>, StoreError> {
    let mut rows: Vec<Value> = Records::new(graph, CASH_ACCOUNT)
        .list(scope, 500)?
        .into_iter()
        .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
        .collect();
    rows.sort_by_key(|row| {
        row.get("created_at")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    });
    Ok(rows)
}

/// `/accounts/{company_id}`: the company's accounts.
pub fn accounts<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let rows = company_accounts(graph, scope, company_id)?;
    let mut out = Vec::new();
    for row in &rows {
        out.push(account_json(row)?);
    }
    Ok(json!({"company_id": company_id, "accounts": out}))
}

/// The suggestion rules, unchanged from the service.
fn compute_suggestions(company_id: &str, accounts: &[Account]) -> Vec<Value> {
    let mut suggestions = Vec::new();
    if accounts.is_empty() {
        return suggestions;
    }
    let threshold_operating = Money::from_i64(10000);
    let threshold_reserve = Money::from_i64(50000);
    let zero = Money::zero();

    for account in accounts {
        let excess = account.excess();

        if excess > threshold_operating && account.kind == "operating" {
            let investments: Vec<&Account> =
                accounts.iter().filter(|a| a.kind == "investment").collect();
            if let Some(best) = investments
                .iter()
                .fold(None::<&&Account>, |best, candidate| match best {
                    Some(current) if candidate.rate > current.rate => Some(candidate),
                    Some(current) => Some(current),
                    None => Some(candidate),
                })
            {
                let benefit = excess.times(&best.rate);
                let apr = best.rate.times(&Money::from_i64(100)).round(1).to_fixed(1);
                suggestions.push(suggestion_json(
                    &Value::Null,
                    company_id,
                    &account.name,
                    &best.name,
                    &excess,
                    &benefit,
                    &format!(
                        "Move excess operating cash to higher-yield investment account ({apr}% APR)"
                    ),
                    "high",
                ));
            }
        }

        if excess > threshold_reserve && account.kind == "reserve" {
            let better: Vec<&Account> = accounts
                .iter()
                .filter(|a| a.kind == "investment" && a.rate > account.rate)
                .collect();
            if let Some(best) =
                better
                    .iter()
                    .fold(None::<&&Account>, |best, candidate| match best {
                        Some(current) if candidate.rate > current.rate => Some(candidate),
                        Some(current) => Some(current),
                        None => Some(candidate),
                    })
            {
                let diff = best.rate.minus(&account.rate);
                let benefit = excess.times(&diff);
                let improvement = diff.times(&Money::from_i64(100)).round(1).to_fixed(1);
                suggestions.push(suggestion_json(
                    &Value::Null,
                    company_id,
                    &account.name,
                    &best.name,
                    &excess,
                    &benefit,
                    &format!(
                        "Transfer to higher-yield account for {improvement}% rate improvement"
                    ),
                    "medium",
                ));
            }
        }
    }

    for account in accounts {
        let underfunded = account.balance < account.minimum
            && matches!(account.kind.as_str(), "reserve" | "tax" | "payroll");
        if !underfunded {
            continue;
        }
        let deficit = account.minimum.minus(&account.balance);
        let need = account.minimum.plus(&deficit);
        let sources: Vec<&Account> = accounts
            .iter()
            .filter(|a| a.kind == "operating" && a.balance > need)
            .collect();
        if let Some(source) = sources
            .iter()
            .fold(None::<&&Account>, |best, candidate| match best {
                Some(current) if candidate.excess() > current.excess() => Some(candidate),
                Some(current) => Some(current),
                None => Some(candidate),
            })
        {
            suggestions.push(suggestion_json(
                &Value::Null,
                company_id,
                &source.name,
                &account.name,
                &deficit,
                &zero,
                &format!(
                    "Top up {} account to meet minimum requirement",
                    account.kind
                ),
                "high",
            ));
        }
    }

    suggestions
}

/// `/optimize/{company_id}`: recompute the suggestions and replace the stored set.
pub fn optimize<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let rows = company_accounts(graph, scope, company_id)?;
    let accounts = accounts_of(&rows)?;
    let suggestions = compute_suggestions(company_id, &accounts);

    let records = Records::new(graph, CASH_SUGGESTION);
    for previous in records.list(scope, 500)? {
        let same_company = previous.get("company_id").and_then(Value::as_str) == Some(company_id);
        if let (true, Some(id)) = (same_company, previous.get("id").and_then(Value::as_str)) {
            records.delete(scope, id)?;
        }
    }

    let mut benefit_total = Money::zero();
    let mut published = Vec::new();
    for suggestion in &suggestions {
        let stored = records.create(
            scope,
            &json!({
                "company_id": company_id,
                "from_account": suggestion["from_account"],
                "to_account": suggestion["to_account"],
                "amount": suggestion["amount"],
                "reason": suggestion["reason"],
                "expected_benefit": suggestion["expected_benefit"],
                "priority": suggestion["priority"],
                "created_at": suggestion["created_at"],
            }),
        )?;
        let benefit = stored_amount(&stored, "expected_benefit")?;
        benefit_total = benefit_total.plus(&benefit);
        published.push(suggestion_json(
            &stored,
            company_id,
            suggestion["from_account"].as_str().unwrap_or(""),
            suggestion["to_account"].as_str().unwrap_or(""),
            &stored_amount(&stored, "amount")?,
            &benefit,
            suggestion["reason"].as_str().unwrap_or(""),
            suggestion["priority"].as_str().unwrap_or(""),
        ));
    }

    Ok(json!({
        "company_id": company_id,
        "suggestions": published,
        "total_count": published.len(),
        "potential_annual_benefit": wire(&benefit_total),
    }))
}

/// `/suggestions/{company_id}`: the stored suggestions, oldest first.
pub fn suggestions<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let mut rows: Vec<Value> = Records::new(graph, CASH_SUGGESTION)
        .list(scope, 500)?
        .into_iter()
        .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
        .collect();
    rows.sort_by_key(|row| {
        row.get("created_at")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    });
    let mut out = Vec::new();
    for row in &rows {
        out.push(json!({
            "id": row.get("id").cloned().unwrap_or(Value::Null),
            "company_id": company_id,
            "from_account": row.get("from_account").cloned().unwrap_or(Value::Null),
            "to_account": row.get("to_account").cloned().unwrap_or(Value::Null),
            "amount": wire(&stored_amount(row, "amount")?),
            "reason": row.get("reason").cloned().unwrap_or(Value::Null),
            "expected_benefit": wire(&stored_amount(row, "expected_benefit")?),
            "priority": row.get("priority").cloned().unwrap_or(Value::Null),
            "created_at": row.get("created_at").cloned().unwrap_or(Value::Null),
        }));
    }
    Ok(json!({"company_id": company_id, "suggestions": out}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(name: &str, kind: &str, balance: i64, minimum: i64, rate: &str) -> Account {
        Account {
            name: name.to_owned(),
            kind: kind.to_owned(),
            balance: Money::from_i64(balance),
            minimum: Money::from_i64(minimum),
            rate: Money::parse(rate).unwrap(),
        }
    }

    #[test]
    fn excess_operating_cash_moves_to_the_best_yielding_investment() {
        let accounts = vec![
            account("Ops", "operating", 60000, 10000, "0.01"),
            account("Inv", "investment", 0, 0, "0.05"),
            account("Inv2", "investment", 0, 0, "0.03"),
        ];
        let out = compute_suggestions("c-1", &accounts);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["from_account"], json!("Ops"));
        assert_eq!(out[0]["to_account"], json!("Inv"));
        assert_eq!(out[0]["amount"], json!(50000.0));
        assert_eq!(out[0]["expected_benefit"], json!(2500.0));
        assert_eq!(
            out[0]["reason"],
            json!("Move excess operating cash to higher-yield investment account (5.0% APR)")
        );
        assert_eq!(out[0]["priority"], json!("high"));
    }

    #[test]
    fn a_small_operating_cushion_is_left_alone() {
        let accounts = vec![
            account("Ops", "operating", 20000, 10000, "0"),
            account("Inv", "investment", 0, 0, "0.05"),
        ];
        assert!(compute_suggestions("c-1", &accounts).is_empty());
    }

    #[test]
    fn an_underfunded_tax_account_is_topped_up_from_the_largest_cushion() {
        let accounts = vec![
            account("Ops", "operating", 60000, 10000, "0"),
            account("Ops2", "operating", 90000, 10000, "0"),
            account("Tax", "tax", 1000, 5000, "0"),
        ];
        let out = compute_suggestions("c-1", &accounts);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["from_account"], json!("Ops2"));
        assert_eq!(out[0]["amount"], json!(4000.0));
        assert_eq!(
            out[0]["reason"],
            json!("Top up tax account to meet minimum requirement")
        );
        assert_eq!(out[0]["expected_benefit"], json!(0.0));
    }

    #[test]
    fn reserve_cash_only_moves_for_a_better_rate() {
        let accounts = vec![
            account("Res", "reserve", 100000, 20000, "0.03"),
            account("Inv", "investment", 0, 0, "0.05"),
        ];
        let out = compute_suggestions("c-1", &accounts);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["priority"], json!("medium"));
        assert_eq!(out[0]["amount"], json!(80000.0));
        // 80000 x (0.05 - 0.03)
        assert_eq!(out[0]["expected_benefit"], json!(1600.0));
        assert_eq!(
            out[0]["reason"],
            json!("Transfer to higher-yield account for 2.0% rate improvement")
        );
    }
}
