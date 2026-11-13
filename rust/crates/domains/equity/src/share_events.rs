#![forbid(unsafe_code)]
//! bonus-shares-service and right-issues-service, on the store.
//!
//! Both hand existing shareholders more shares, and neither takes money in at the moment of issue:
//! a bonus issue capitalises a reserve, and a rights issue offers new shares at a set price that are
//! only paid for when each holder accepts.
//!
//! A bonus issue moves nominal value out of the named reserve into share capital, so the total is
//! the shares times their nominal value and the amount utilized is that same total - nothing is
//! released and kept.
//!
//! A rights issue carries an entitlement per shareholder and fills up as acceptances and
//! renunciations arrive. It completes when every entitlement is covered.
//!
//! Kept as shipped, because these are the service's own rules and a client depends on them:
//!
//! * A reserve name it does not recognise falls back to retained earnings (`3300`), including an
//!   empty one. The three it knows are `share_premium` (3210), `retained_earnings` (3300) and
//!   `general_reserve` (3310).
//! * Completion counts one unit per renunciation rather than the shares renounced. A holder who
//!   renounces fifty shares clears one unit of entitlement, not fifty. This is the service's
//!   arithmetic and is pinned here rather than corrected, because correcting it would silently
//!   complete issues that the old service left open.
//! * Accepting more shares than were offered is not refused; the acceptance is recorded as given.
//!   A rights issue that records 900 accepted against an entitlement of 100 is a records problem the
//!   books should show, not one this layer should quietly cap.
//! * The renunciation entry debits and credits the same bank account for the consideration, so it
//!   balances to zero and records the transfer without claiming new money arrived.

use serde_json::{json, Map, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::read::{text, whole_signed};
use vimbai_store::{BookScope, Graph, Records, StoreError, BONUS_ISSUE, RENUNCIATION, RIGHT_ISSUE};

fn flag(payload: &Value, key: &str, fallback: bool) -> bool {
    payload
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(fallback)
}

fn ledger(date: &str, description: String, reference: String, lines: Vec<Value>) -> Value {
    json!({
        "date": date,
        "description": description,
        "entries": lines,
        "reference": reference,
    })
}

fn line(code: &str, name: &str, debit: &Money, credit: &Money) -> Value {
    json!({
        "account_code": code,
        "description": name,
        "debit": wire(debit),
        "credit": wire(credit),
    })
}

fn zero() -> Money {
    Money::zero()
}

/// The reserve a bonus issue draws on. An unknown name draws on retained earnings, as the service's
/// own lookup did.
pub fn bonus_reserve_account(source_reserve: &str) -> &'static str {
    match source_reserve {
        "share_premium" => "3210",
        "general_reserve" => "3310",
        _ => "3300",
    }
}

/// `/issue`: capitalise a reserve into share capital.
pub fn issue_bonus_shares<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let shares = whole_signed(payload, "shares_issued")?;
    let nominal_value = amount_or_zero(payload, "nominal_value")?;
    let total = nominal_value.times(&Money::from_i64(shares));
    let reserve = text(payload, "source_reserve");
    let account = bonus_reserve_account(&reserve);
    let date = text(payload, "issue_date");

    let journal = ledger(
        &date,
        format!("Issue of {shares} bonus shares from {reserve}"),
        format!("BONUS-{}", &text(payload, "id")),
        vec![
            line(account, &format!("{reserve} Reserve"), &total, &zero()),
            line("3200", "Share Capital", &zero(), &total),
        ],
    );

    let allocations = payload
        .get("shareholder_allocations")
        .cloned()
        .unwrap_or_else(|| json!({}));

    Records::new(graph, BONUS_ISSUE).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "issue_date": date,
            "shares_issued": shares,
            "nominal_value": wire(&nominal_value),
            "source_reserve": reserve,
            "shareholder_allocations": allocations,
            "total_nominal_value": wire(&total),
            // the whole nominal amount leaves the reserve; nothing is retained
            "amount_utilized": wire(&total),
            "journal_entry": journal,
        }),
    )
}

pub fn list_bonus_issues<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, BONUS_ISSUE).list(scope, 1000)?;
    let filtered: Vec<Value> = match company_id {
        Some(company) => rows
            .into_iter()
            .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company))
            .collect(),
        None => rows,
    };
    Ok(json!({ "issues": filtered }))
}

/// `/offer`: open a rights issue against a set of entitlements.
pub fn offer_rights<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let offered = whole_signed(payload, "new_shares_offered")?;
    let price = amount_or_zero(payload, "issue_price")?;
    // the offer's stated total is the new shares at the issue price
    let total = price.times(&Money::from_i64(offered));

    Records::new(graph, RIGHT_ISSUE).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "issue_date": text(payload, "issue_date"),
            "expiry_date": text(payload, "expiry_date"),
            "share_class": text(payload, "share_class"),
            "rights_offered": whole_signed(payload, "rights_offered")?,
            "new_shares_offered": offered,
            "issue_price": wire(&price),
            "nominal_value": wire(&amount_or_zero(payload, "nominal_value")?),
            "total_proceeds": wire(&total),
            "renouncement_allowed": flag(payload, "renouncement_allowed", true),
            "entitlements": payload.get("entitlements").cloned().unwrap_or_else(|| json!({})),
            "acceptances": json!({}),
            "renunciations": json!({}),
            "status": "open",
        }),
    )
}

/// Completed once every entitlement is covered by acceptances plus renunciations.
///
/// The renunciation count is per shareholder, not per share: a map of original to new holder gives
/// one unit each. That is the service's arithmetic and it is kept.
fn status_after_changes(issue: &Value) -> String {
    let status = issue
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("open")
        .to_string();
    let entitlements = issue
        .get("entitlements")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let acceptances = issue
        .get("acceptances")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let renunciations = issue
        .get("renunciations")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    for (shareholder, entitled) in &entitlements {
        let renounced = renunciations.contains_key(shareholder) as i64;
        let accepted = acceptances
            .get(shareholder)
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let entitled = entitled.as_i64().unwrap_or(0);
        if entitled != accepted + renounced {
            return status;
        }
    }
    "completed".to_string()
}

/// `/{id}/accept`: a holder takes up some of their entitlement.
pub fn accept_rights<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    issue_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, RIGHT_ISSUE);
    let issue = records.get(scope, issue_id)?;
    let shareholder = text(payload, "shareholder_id");
    let shares = whole_signed(payload, "shares_accepted")?;

    let mut acceptances: Map<String, Value> = issue
        .get("acceptances")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    acceptances.insert(shareholder, json!(shares));

    let updated = json!({
        "acceptances": Value::Object(acceptances),
        "status": "",
    });
    let mut candidate = issue.clone();
    candidate["acceptances"] = updated["acceptances"].clone();
    let status = status_after_changes(&candidate);

    let price = stored_amount(&issue, "issue_price")?;
    let nominal_value = stored_amount(&issue, "nominal_value")?;
    let paid = price.times(&Money::from_i64(shares));
    let nominal = nominal_value.times(&Money::from_i64(shares));
    let premium = paid.minus(&nominal);

    let journal = ledger(
        "",
        format!("Acceptance of rights issue - {shares} shares"),
        format!("RIGHT-ACCEPT-{issue_id}"),
        vec![
            line("1000", "Bank", &paid, &zero()),
            line("3200", "Share Capital", &zero(), &nominal),
            line("3210", "Share Premium", &zero(), &premium),
        ],
    );

    records.update(
        scope,
        issue_id,
        &json!({
            "acceptances": updated["acceptances"].clone(),
            "status": status,
            "last_acceptance_journal_entry": journal,
        }),
    )
}

/// `/{id}/renounce`: a holder passes their rights to someone else.
pub fn renounce_rights<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    issue_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, RIGHT_ISSUE);
    let issue = records.get(scope, issue_id)?;

    if !flag(&issue, "renouncement_allowed", true) {
        return Err(StoreError::Invalid(
            "Renouncement not allowed for this issue".into(),
        ));
    }

    let from = text(payload, "shareholder_id");
    let to = text(payload, "new_shareholder_id");
    let price = amount_or_zero(payload, "renunciation_price")?;

    let mut renunciations: Map<String, Value> = issue
        .get("renunciations")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    renunciations.insert(from.clone(), json!(to));

    let mut candidate = issue.clone();
    candidate["renunciations"] = Value::Object(renunciations.clone());
    let status = status_after_changes(&candidate);

    let renunciation = Records::new(graph, RENUNCIATION).create(
        scope,
        &json!({
            "right_issue_id": issue_id,
            "original_shareholder_id": from,
            "new_shareholder_id": to,
            "renunciation_date": text(payload, "renunciation_date"),
            "renunciation_price": wire(&price),
            // the consideration changes hands between the two holders, so it debits and credits the
            // same bank account and records no new money
            "journal_entry": ledger(
                &text(payload, "renunciation_date"),
                "Rights renunciation payment".to_string(),
                format!("RIGHT-REN-{issue_id}"),
                vec![
                    line("1000", "Bank", &price, &zero()),
                    line("1000", "Bank", &zero(), &price),
                ],
            ),
        }),
    )?;

    records.update(
        scope,
        issue_id,
        &json!({
            "renunciations": Value::Object(renunciations),
            "status": status,
        }),
    )?;

    Ok(json!({
        "right_issue": {
            "status": status,
            "renunciations": candidate["renunciations"].clone(),
        },
        "renunciation": renunciation,
    }))
}

pub fn list_right_issues<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, RIGHT_ISSUE).list(scope, 1000)?;
    let filtered: Vec<Value> = match company_id {
        Some(company) => rows
            .into_iter()
            .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company))
            .collect(),
        None => rows,
    };
    Ok(json!({ "right_issues": filtered }))
}

pub fn get_right_issue<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    issue_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, RIGHT_ISSUE).get(scope, issue_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_bonus_issue_moves_the_whole_nominal_amount_out_of_the_reserve() {
        let graph = FakeGraph::returning(vec![json!({"id": "b-1"})]);
        issue_bonus_shares(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1", "shares_issued": 1000, "nominal_value": "0.50",
                "source_reserve": "share_premium", "shareholder_allocations": {"s-1": 600}
            }),
        )
        .expect("issued");
        let (_, params) = graph.last();
        let written = &params["props"];
        assert_eq!(written["total_nominal_value"], json!("500.000000"));
        assert_eq!(written["amount_utilized"], json!("500.000000"));
        let lines = written["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["account_code"], json!("3210"));
        assert_eq!(lines[0]["debit"], json!("500.000000"));
        assert_eq!(lines[1]["credit"], json!("500.000000"));
    }

    #[test]
    fn an_unknown_reserve_draws_on_retained_earnings() {
        assert_eq!(bonus_reserve_account("general_reserve"), "3310");
        assert_eq!(bonus_reserve_account("retained_earnings"), "3300");
        assert_eq!(bonus_reserve_account(""), "3300");
        assert_eq!(bonus_reserve_account("revaluation_reserve"), "3300");
    }

    #[test]
    fn a_rights_offer_totals_the_new_shares_at_the_issue_price() {
        let graph = FakeGraph::returning(vec![json!({"id": "r-1"})]);
        offer_rights(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1", "rights_offered": 2000, "new_shares_offered": 1000,
                "issue_price": "1.25", "nominal_value": "1", "entitlements": {"s-1": 600, "s-2": 400}
            }),
        )
        .expect("offered");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["total_proceeds"], json!("1250.000000"));
        assert_eq!(params["props"]["status"], json!("open"));
        assert_eq!(params["props"]["renouncement_allowed"], json!(true));
        assert_eq!(params["props"]["entitlements"]["s-2"], json!(400));
    }

    #[test]
    fn an_acceptance_splits_the_money_into_nominal_and_premium() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "r-1", "issue_price": "1.25", "nominal_value": "1",
            "entitlements": {"s-1": 600}, "acceptances": {}, "renunciations": {},
            "renouncement_allowed": true, "status": "open"
        })]);
        accept_rights(
            &graph,
            &scope(),
            "r-1",
            &json!({"shareholder_id": "s-1", "shares_accepted": 600}),
        )
        .expect("accepted");
        let (_, params) = graph.last();
        assert_eq!(params["patch"]["acceptances"]["s-1"], json!(600));
        // 600 of 600 entitled, nothing renounced, so the issue closes
        assert_eq!(params["patch"]["status"], json!("completed"));
        let lines = params["patch"]["last_acceptance_journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["debit"], json!("750.000000"));
        assert_eq!(lines[1]["credit"], json!("600.000000"));
        assert_eq!(lines[2]["credit"], json!("150.000000"));
    }

    #[test]
    fn an_issue_stays_open_while_an_entitlement_is_uncovered() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "r-1", "issue_price": "1", "nominal_value": "1",
            "entitlements": {"s-1": 600, "s-2": 400}, "acceptances": {}, "renunciations": {},
            "status": "open"
        })]);
        accept_rights(
            &graph,
            &scope(),
            "r-1",
            &json!({"shareholder_id": "s-1", "shares_accepted": 600}),
        )
        .expect("accepted");
        let (_, params) = graph.last();
        assert_eq!(params["patch"]["status"], json!("open"));
    }

    #[test]
    fn a_renunciation_clears_one_unit_of_entitlement_however_many_shares_it_covers() {
        // this is the service's own arithmetic: a renunciation entry counts as one, not as the
        // shares renounced, so 399 shares renounced against a 400 entitlement leaves it open
        let graph = FakeGraph::returning(vec![json!({
            "id": "r-1", "issue_price": "1", "nominal_value": "1",
            "entitlements": {"s-1": 400}, "acceptances": {}, "renunciations": {},
            "renouncement_allowed": true, "status": "open"
        })]);
        renounce_rights(
            &graph,
            &scope(),
            "r-1",
            &json!({
                "shareholder_id": "s-1", "new_shareholder_id": "s-9",
                "renunciation_price": "0.20", "renunciation_date": "2026-03-02"
            }),
        )
        .expect("renounced");
        let (_, params) = graph.last();
        assert_eq!(params["patch"]["renunciations"]["s-1"], json!("s-9"));
        // entitlement 400 vs one unit renounced, so it stays open
        assert_eq!(params["patch"]["status"], json!("open"));
    }

    #[test]
    fn a_renunciation_of_a_one_share_entitlement_completes_the_issue() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "r-1", "issue_price": "1", "nominal_value": "1",
            "entitlements": {"s-1": 1}, "acceptances": {}, "renunciations": {},
            "renouncement_allowed": true, "status": "open"
        })]);
        renounce_rights(
            &graph,
            &scope(),
            "r-1",
            &json!({"shareholder_id": "s-1", "new_shareholder_id": "s-9", "renunciation_price": "0.20"}),
        )
        .expect("renounced");
        let (_, params) = graph.last();
        assert_eq!(params["patch"]["status"], json!("completed"));
    }

    #[test]
    fn a_closed_issue_refuses_renunciation() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "r-1", "renouncement_allowed": false, "status": "open"
        })]);
        let error = renounce_rights(
            &graph,
            &scope(),
            "r-1",
            &json!({"shareholder_id": "s-1", "new_shareholder_id": "s-9"}),
        )
        .expect_err("refused");
        assert_eq!(error.to_string(), "Renouncement not allowed for this issue");
    }

    #[test]
    fn renunciation_consideration_balances_to_zero() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "r-1", "issue_price": "1", "nominal_value": "1",
            "entitlements": {"s-1": 5}, "acceptances": {}, "renunciations": {},
            "renouncement_allowed": true, "status": "open"
        })]);
        renounce_rights(
            &graph,
            &scope(),
            "r-1",
            &json!({"shareholder_id": "s-1", "new_shareholder_id": "s-9", "renunciation_price": "12.34"}),
        )
        .expect("renounced");
        let (_, write) = graph.statement(1);
        let lines = write["props"]["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["debit"], json!("12.340000"));
        assert_eq!(lines[1]["credit"], json!("12.340000"));
    }
}
