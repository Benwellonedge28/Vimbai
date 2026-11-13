#![forbid(unsafe_code)]
//! retained-profits-service, on the store.
//!
//! Three records that all land on the same account: an appropriation of the year's profit, an
//! interim dividend taken out during the year, and a prior-year adjustment correcting an earlier one.
//!
//! The appropriation is the arithmetic that matters, and it is a chain:
//!
//! * total appropriated is everything set aside,
//! * what is carried forward is the profit less what was set aside,
//! * closing retained earnings is what was open plus what was carried forward.
//!
//! All three figures are exact, so a company with more retained earnings than a float can hold keeps
//! every digit.
//!
//! Kept as shipped:
//!
//! * Appropriations are recorded as `approved` without an approval step.
//! * Appropriating more than the year's profit is not refused; the balance carried forward is simply
//!   negative, which is a loss, and the service reported it as one.
//! * A positive prior-year adjustment debits retained earnings and credits receivables; a negative one
//!   does the reverse. The sign flip is the service's own convention for a credit note.
//! * The summary adds carried-forward balances to the adjustments for the company and reports the
//!   total. It reads every appropriation the Book holds, so a summary is over this caller's records
//!   only.
//!
//! One deliberate change: paying an interim dividend that does not exist is a refusal. The service
//! answered 200 with `{"error": "Dividend not found"}`, which a status-checking client reads as paid.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::read::text;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, INTERIM_DIVIDEND, PRIOR_YEAR_ADJUSTMENT,
    PROFIT_APPROPRIATION,
};

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

/// The three figures an appropriation produces from one year's profit.
pub fn appropriation_totals(
    net_profit: &Money,
    opening: &Money,
    allocations: &[(String, Money)],
) -> (Money, Money, Money) {
    let mut total = Money::zero();
    for (_, amount) in allocations {
        total = total.plus(amount);
    }
    let carried = net_profit.minus(&total);
    let closing = opening.plus(&carried);
    (total, carried, closing)
}

fn allocations_from(payload: &Value) -> Result<Vec<(String, Money)>, StoreError> {
    let map = payload
        .get("appropriations")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for (kind, amount) in map {
        let parsed = match &amount {
            Value::String(raw) => Money::parse(raw).map_err(|_| {
                StoreError::Invalid(format!("appropriations.{kind} is not an amount"))
            })?,
            Value::Number(number) => {
                Money::from_f64_shortest(number.as_f64().ok_or_else(|| {
                    StoreError::Invalid(format!("appropriations.{kind} is not an amount"))
                })?)
                .map_err(|_| {
                    StoreError::Invalid(format!("appropriations.{kind} is not an amount"))
                })?
            }
            _ => {
                return Err(StoreError::Invalid(format!(
                    "appropriations.{kind} is not an amount"
                )))
            }
        };
        out.push((kind, parsed));
    }
    out.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(out)
}

/// `/appropriations/create`.
pub fn create_appropriation<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let net_profit = amount_or_zero(payload, "net_profit")?;
    let opening = amount_or_zero(payload, "opening_retained_earnings")?;
    let allocations = allocations_from(payload)?;
    let (total, carried, closing) = appropriation_totals(&net_profit, &opening, &allocations);
    let year = text(payload, "financial_year");
    let period_end = text(payload, "period_end");

    let mut lines = vec![line("3300", "Retained Earnings", &net_profit, &zero())];
    for (kind, amount) in &allocations {
        lines.push(line(
            "3300",
            &format!("Retained Earnings ({kind})"),
            &zero(),
            amount,
        ));
    }

    let journal = ledger(
        &period_end,
        format!("Profit appropriation for {year}"),
        "APPROP-PENDING".to_string(),
        lines,
    );

    let map: serde_json::Map<String, Value> = allocations
        .iter()
        .map(|(kind, amount)| (kind.clone(), json!(wire(amount))))
        .collect();

    Records::new(graph, PROFIT_APPROPRIATION).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "financial_year": year,
            "period_start": text(payload, "period_start"),
            "period_end": period_end,
            "net_profit": wire(&net_profit),
            "appropriations": Value::Object(map),
            "total_appropriations": wire(&total),
            "balance_carried_forward": wire(&carried),
            "opening_retained_earnings": wire(&opening),
            "closing_retained_earnings": wire(&closing),
            "status": "approved",
            "journal_entry": journal,
        }),
    )
}

/// `/interim-dividends/declare`.
pub fn declare_interim_dividend<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let amount = amount_or_zero(payload, "amount")?;
    let date = text(payload, "declaration_date");

    let journal = ledger(
        &date,
        "Declaration of interim dividend".to_string(),
        "INTERIM-DIV-PENDING".to_string(),
        vec![
            line("3300", "Retained Earnings", &amount, &zero()),
            line("2310", "Dividend Payable", &zero(), &amount),
        ],
    );

    Records::new(graph, INTERIM_DIVIDEND).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "amount": wire(&amount),
            "per_share_amount": wire(&amount_or_zero(payload, "per_share_amount")?),
            "total_shares": payload.get("total_shares").and_then(Value::as_i64).unwrap_or(0),
            "declaration_date": date,
            "status": "declared",
            "journal_entry": journal,
        }),
    )
}

/// `/interim-dividends/{id}/pay`.
pub fn pay_interim_dividend<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    dividend_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, INTERIM_DIVIDEND);
    let dividend = records.get(scope, dividend_id)?;
    let amount = stored_amount(&dividend, "amount")?;
    let paid_on = text(payload, "payment_date");

    let journal = ledger(
        &paid_on,
        "Payment of interim dividend".to_string(),
        format!("INTERIM-DIV-PAY-{dividend_id}"),
        vec![
            line("2310", "Dividend Payable", &amount, &zero()),
            line("1000", "Bank", &zero(), &amount),
        ],
    );

    records.update(
        scope,
        dividend_id,
        &json!({
            "status": "paid",
            "payment_date": paid_on,
            "payment_journal_entry": journal,
        }),
    )
}

/// `/adjustments/create`: a correction to an earlier year, by sign.
pub fn create_prior_year_adjustment<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let amount = amount_or_zero(payload, "amount")?;
    let date = text(payload, "adjustment_date");
    let description = text(payload, "description");

    // a positive adjustment debits retained earnings; a negative one credits it and debits
    // receivables instead
    let (earnings_debit, receivables_credit) = if amount > Money::zero() {
        (amount.clone(), amount.clone())
    } else {
        (zero(), zero())
    };
    let (earnings_credit, receivables_debit) = if amount > Money::zero() {
        (zero(), zero())
    } else {
        (amount.abs(), amount.abs())
    };

    let journal = ledger(
        &date,
        format!("Prior year adjustment: {description}"),
        "PYA-PENDING".to_string(),
        vec![
            line(
                "3300",
                "Retained Earnings",
                &earnings_debit,
                &earnings_credit,
            ),
            line(
                "1100",
                "Trade Receivables",
                &receivables_debit,
                &receivables_credit,
            ),
        ],
    );

    Records::new(graph, PRIOR_YEAR_ADJUSTMENT).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "adjustment_type": text(payload, "adjustment_type"),
            "description": description,
            "amount": wire(&amount),
            "affected_year": text(payload, "affected_year"),
            "adjustment_date": date,
            "journal_entry": journal,
        }),
    )
}

pub fn list_appropriations<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
    financial_year: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, PROFIT_APPROPRIATION).list(scope, 1000)?;
    let filtered: Vec<Value> = rows
        .into_iter()
        .filter(|row| match company_id {
            Some(company) => row.get("company_id").and_then(Value::as_str) == Some(company),
            None => true,
        })
        .filter(|row| match financial_year {
            Some(year) => row.get("financial_year").and_then(Value::as_str) == Some(year),
            None => true,
        })
        .collect();
    Ok(json!({ "appropriations": filtered }))
}

pub fn list_interim_dividends<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, INTERIM_DIVIDEND).list(scope, 1000)?;
    let filtered: Vec<Value> = match company_id {
        Some(company) => rows
            .into_iter()
            .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company))
            .collect(),
        None => rows,
    };
    Ok(json!({ "interim_dividends": filtered }))
}

pub fn list_adjustments<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, PRIOR_YEAR_ADJUSTMENT).list(scope, 1000)?;
    let filtered: Vec<Value> = match company_id {
        Some(company) => rows
            .into_iter()
            .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company))
            .collect(),
        None => rows,
    };
    Ok(json!({ "adjustments": filtered }))
}

/// `/summary/{company_id}`: carried forward plus adjustments.
pub fn retained_earnings_summary<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let mine = |rows: Vec<Value>| -> Vec<Value> {
        rows.into_iter()
            .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
            .collect()
    };
    let appropriations = mine(Records::new(graph, PROFIT_APPROPRIATION).list(scope, 1000)?);
    let adjustments = mine(Records::new(graph, PRIOR_YEAR_ADJUSTMENT).list(scope, 1000)?);

    let mut carried = Money::zero();
    for row in &appropriations {
        carried = carried.plus(&stored_amount(row, "balance_carried_forward")?);
    }
    let mut total_adjustments = Money::zero();
    for row in &adjustments {
        total_adjustments = total_adjustments.plus(&stored_amount(row, "amount")?);
    }
    let net = carried.plus(&total_adjustments);

    Ok(json!({
        "company_id": company_id,
        "total_carried_forward": wire(&carried),
        "total_prior_year_adjustments": wire(&total_adjustments),
        "net_retained_earnings": wire(&net),
        "last_appropriation": appropriations.last().cloned().unwrap_or(Value::Null),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn an_appropriation_carries_the_rest_forward() {
        let alloc = vec![
            ("dividend".to_string(), Money::parse("200").unwrap()),
            ("reserve".to_string(), Money::parse("300").unwrap()),
        ];
        let (total, carried, closing) = appropriation_totals(
            &Money::parse("1000").unwrap(),
            &Money::parse("5000").unwrap(),
            &alloc,
        );
        assert_eq!(wire(&total), "500.000000");
        assert_eq!(wire(&carried), "500.000000");
        assert_eq!(wire(&closing), "5500.000000");
    }

    #[test]
    fn appropriating_more_than_the_profit_carries_a_loss_forward() {
        let alloc = vec![("dividend".to_string(), Money::parse("1200").unwrap())];
        let (_, carried, closing) = appropriation_totals(
            &Money::parse("1000").unwrap(),
            &Money::parse("5000").unwrap(),
            &alloc,
        );
        assert_eq!(wire(&carried), "-200.000000");
        assert_eq!(wire(&closing), "4800.000000");
    }

    #[test]
    fn a_huge_profit_keeps_every_digit_through_the_chain() {
        let alloc = vec![("reserve".to_string(), Money::parse("0.45").unwrap())];
        let (total, carried, closing) = appropriation_totals(
            &Money::parse("123456789012345678901.55").unwrap(),
            &Money::parse("987654321098765432109.55").unwrap(),
            &alloc,
        );
        assert_eq!(wire(&total), "0.450000");
        assert_eq!(wire(&carried), "123456789012345678901.100000");
        assert_eq!(wire(&closing), "1111111110111111111010.650000");
    }

    #[test]
    fn the_appropriation_entry_debits_profit_and_credits_each_allocation() {
        let graph = FakeGraph::returning(vec![json!({"id": "a-1"})]);
        create_appropriation(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1", "financial_year": "2026",
                "period_end": "2026-12-31", "net_profit": "1000",
                "opening_retained_earnings": "5000",
                "appropriations": {"reserve": "300", "dividend": "200"}
            }),
        )
        .expect("created");
        let (_, params) = graph.last();
        let written = &params["props"];
        assert_eq!(written["total_appropriations"], json!("500.000000"));
        assert_eq!(written["closing_retained_earnings"], json!("5500.000000"));
        let lines = written["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["debit"], json!("1000.000000"));
        assert_eq!(lines[1]["credit"], json!("200.000000"));
        assert_eq!(lines[2]["credit"], json!("300.000000"));
    }

    #[test]
    fn a_positive_adjustment_debits_earnings_and_a_negative_one_credits_them() {
        let graph = FakeGraph::returning(vec![json!({"id": "p-1"})]);
        create_prior_year_adjustment(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "amount": "250", "description": "understated sales"}),
        )
        .expect("created");
        let (_, params) = graph.last();
        let lines = params["props"]["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["debit"], json!("250.000000"));
        assert_eq!(lines[0]["credit"], json!("0.000000"));

        let credit = FakeGraph::returning(vec![json!({"id": "p-2"})]);
        create_prior_year_adjustment(
            &credit,
            &scope(),
            &json!({"company_id": "co-1", "amount": "-250", "description": "credit note"}),
        )
        .expect("created");
        let (_, params) = credit.last();
        let lines = params["props"]["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["debit"], json!("0.000000"));
        assert_eq!(lines[0]["credit"], json!("250.000000"));
        assert_eq!(lines[1]["debit"], json!("250.000000"));
    }

    #[test]
    fn the_summary_adds_carried_forward_to_the_adjustments() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "a-1", "company_id": "co-1", "balance_carried_forward": "500"}),
                json!({"id": "a-2", "company_id": "co-2", "balance_carried_forward": "999"}),
                json!({"id": "a-3", "company_id": "co-1", "balance_carried_forward": "250.25"}),
            ],
            vec![json!({"id": "p-1", "company_id": "co-1", "amount": "-50.25"})],
        ]);
        let answer = retained_earnings_summary(&graph, &scope(), "co-1").expect("summary");
        assert_eq!(answer["total_carried_forward"], json!("750.250000"));
        assert_eq!(answer["total_prior_year_adjustments"], json!("-50.250000"));
        assert_eq!(answer["net_retained_earnings"], json!("700.000000"));
        // another company's balance never enters
        assert_eq!(answer["company_id"], json!("co-1"));
    }

    #[test]
    fn an_empty_summary_reports_zero_rather_than_nothing() {
        let graph = FakeGraph::queue(vec![Vec::new(), Vec::new()]);
        let answer = retained_earnings_summary(&graph, &scope(), "co-9").expect("summary");
        assert_eq!(answer["net_retained_earnings"], json!("0.000000"));
        assert_eq!(answer["last_appropriation"], Value::Null);
    }
}

#[cfg(test)]
mod float_history {
    //! `calculations/equity_payroll` carried a float `retained_profits` module with the same two
    //! functions. Nothing reached it - no route, no seam wrapper, no re-export - so it was deleted
    //! rather than converted, and this test records the one case the two would have disagreed on, so
    //! the deletion is a decision with evidence behind it.
    use vimbai_common::Money;

    #[test]
    fn the_exact_chain_disagrees_with_a_float_one_only_where_it_must() {
        // 0.1 + 0.2 in floats is 0.30000000000000004, so a float chain that appropriated a tenth and
        // a fifth of a currency unit would carry forward 999.7 as 999.6999999999999
        let alloc = vec![
            ("a".to_string(), Money::parse("0.1").unwrap()),
            ("b".to_string(), Money::parse("0.2").unwrap()),
        ];
        let (total, carried, closing) = super::appropriation_totals(
            &Money::parse("1000").unwrap(),
            &Money::parse("0").unwrap(),
            &alloc,
        );
        assert_eq!(vimbai_common::Money::to_fixed(&total, 2), "0.30");
        assert_eq!(vimbai_common::Money::to_fixed(&carried, 2), "999.70");
        assert_eq!(vimbai_common::Money::to_fixed(&closing, 2), "999.70");
        let float_total: f64 = 0.1 + 0.2;
        assert_ne!(format!("{float_total:.17}"), "0.30000000000000000");
    }
}
