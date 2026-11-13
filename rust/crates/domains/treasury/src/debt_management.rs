#![forbid(unsafe_code)]
//! debt-management-service: the loans a company carries, the schedule that pays each one down, and the summary
//! the whole book adds up to.
//!
//! The arithmetic is the service's own, in decimals rather than floats:
//!
//! * a monthly payment is `principal * r * (1+r)^n / ((1+r)^n - 1)` with `r` the annual rate over twelve, and a
//!   zero rate simply divides the principal over the term, because the annuity formula divides by nothing at zero;
//! * a schedule walks the balance period by period, charging interest on what is still owed and putting the rest
//!   of the payment against the principal, and reports a balance of nothing rather than a negative from the last
//!   penny of rounding;
//! * the summary adds the remaining balances, the monthly payments and the interest over the life of the loans,
//!   weights the rate by balance rather than by loan, and reports no ratio rather than dividing by nothing when
//!   there is no debt or no equity.
//!
//! The one departure from exactness is the compounding power, which converges and is taken to the cent where the
//! schedule publishes figures; the totals are exact decimals.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::{BookScope, Graph, Records, StoreError, DEBT_LOAN};

use crate::helpers::{field, money, money_or_nothing, or_default, remember, text, whole};
use vimbai_store::read::whole_signed;

/// `POST /loans`: a loan as it was set up.
pub fn create_loan<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let principal = money(payload, "principal")?;
    if whole(payload, "term_months") == 0 {
        return Err(StoreError::Invalid("term_months must be positive".into()));
    }

    remember(
        graph,
        scope,
        DEBT_LOAN,
        json!({
            "company_id": text(payload, "company_id"),
            "loan_name": text(payload, "loan_name"),
            "lender": or_default(payload, "lender", ""),
            "principal": wire(&principal),
            "interest_rate": wire(&money_or_nothing(payload, "interest_rate")?),
            "term_months": whole(payload, "term_months"),
            "remaining_balance": wire(&money_or_nothing(payload, "remaining_balance")?),
            "currency": or_default(payload, "currency", "USD"),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /loans`: the company's loans, in the order they were set up.
pub fn list_loans<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    Ok(Value::Array(mine(graph, scope, company_id)?))
}

/// `POST /loans/{loan_id}/schedule`: the period-by-period paydown.
pub fn schedule<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = text(payload, "company_id");
    let loan_id = text(payload, "loan_id");
    let Some(loan) = mine(graph, scope, &company_id)?
        .into_iter()
        .find(|row| field(row, "id") == loan_id)
    else {
        return Err(StoreError::NotFound);
    };
    Ok(Value::Array(paydown(&loan)?))
}

/// `GET /summary`: what the loans add up to.
pub fn summary<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
    equity: &Money,
) -> Result<Value, StoreError> {
    let loans = mine(graph, scope, company_id)?;
    let mut total_debt = Money::zero();
    let mut total_monthly = Money::zero();
    let mut total_interest = Money::zero();
    let mut weighted = Money::zero();
    let mut rows = Vec::new();

    for loan in &loans {
        let balance = money_or_nothing(loan, "remaining_balance")?;
        let rate = money_or_nothing(loan, "interest_rate")?;
        let principal = money_or_nothing(loan, "principal")?;
        let months = whole(loan, "term_months");
        let payment = payment(&principal, &rate, months)?;

        total_debt = total_debt.plus(&balance);
        total_monthly = total_monthly.plus(&payment);
        total_interest = total_interest.plus(
            &payment
                .times(&Money::from_u64(months as u64))
                .minus(&principal),
        );
        weighted = weighted.plus(&rate.times(&balance));
        rows.push(json!({
            "id": field(loan, "id"),
            "name": field(loan, "loan_name"),
            "balance": wire(&balance),
            "rate": wire(&rate),
        }));
    }

    // the service published the weighted rate as the decimal it works in, so twelve percent reads 0.12
    let weighted_rate = if total_debt.is_zero() {
        Money::zero()
    } else {
        weighted.over(&total_debt).round(4)
    };
    let debt_to_equity = if equity.is_zero() {
        Money::zero()
    } else {
        total_debt.over(equity).round(4)
    };

    Ok(json!({
        "company_id": company_id,
        "total_debt": wire(&total_debt.round(2)),
        "total_interest": wire(&total_interest.round(2)),
        "total_monthly_payments": wire(&total_monthly.round(2)),
        "debt_to_equity": wire(&debt_to_equity),
        "weighted_avg_rate": wire(&weighted_rate),
        "loans": rows,
    }))
}

/// The payment that clears a principal over its term at a monthly rate.
pub fn payment(principal: &Money, rate: &Money, months: u32) -> Result<Money, StoreError> {
    if months == 0 {
        return Err(StoreError::Invalid("term_months must be positive".into()));
    }
    if rate.is_zero() {
        return Ok(principal.over(&Money::from_u64(months as u64)));
    }
    let monthly = rate.over(&Money::parse("12").expect("money"));
    let one = Money::parse("1").expect("money");
    let growth = one.plus(&monthly).pow(months);
    Ok(principal
        .times(&monthly)
        .times(&growth)
        .over(&growth.minus(&one)))
}

/// One period of a loan's paydown.
#[derive(Debug, Clone)]
pub struct PaydownRow {
    pub period: u32,
    pub payment: Money,
    pub principal_component: Money,
    pub interest_component: Money,
    pub balance: Money,
}

/// The schedule one loan is paid down on: interest on the balance, the rest of the payment against the principal.
///
/// This is the only place a loan paydown is worked out. The stored loans below and the realtime engine's `/amortize`
/// both read it, so a change to how a payment is split reaches both rather than drifting apart.
pub fn paydown_rows(
    principal: &Money,
    rate: &Money,
    months: u32,
) -> Result<Vec<PaydownRow>, StoreError> {
    let each = payment(principal, rate, months)?;
    let monthly = rate.over(&Money::parse("12").expect("money"));

    let mut balance = principal.clone();
    let mut rows = Vec::with_capacity(months as usize);
    for period in 1..=months {
        let interest = balance.times(&monthly);
        let against = each.minus(&interest);
        balance = balance.minus(&against);
        rows.push(PaydownRow {
            period,
            payment: each.round(2),
            principal_component: against.round(2),
            interest_component: interest.round(2),
            // the last penny of rounding must not leave the loan owing less than nothing
            balance: balance.round(2).max_zero(),
        });
    }
    Ok(rows)
}

/// The schedule for one loan, as the service publishes it.
fn paydown(loan: &Value) -> Result<Vec<Value>, StoreError> {
    let principal = money_or_nothing(loan, "principal")?;
    let rate = money_or_nothing(loan, "interest_rate")?;
    let months = whole(loan, "term_months");
    Ok(paydown_rows(&principal, &rate, months)?
        .into_iter()
        .map(|row| {
            json!({
                "period": row.period,
                "payment": wire(&row.payment),
                "principal_component": wire(&row.principal_component),
                "interest_component": wire(&row.interest_component),
                "balance": wire(&row.balance),
            })
        })
        .collect())
}

/// `/amortize`: a loan paid down month by month, in the shape the realtime engine published.
///
/// The term arrives in years here and in months there, and the rate arrives as a percent rather than a decimal, which
/// is the only difference between this door and the stored loans': both walk the same paydown.
pub fn quick_amortisation(payload: &Value) -> Result<Value, StoreError> {
    let principal = money_or_nothing(payload, "principal")?;
    if principal.is_zero() || principal.is_negative() {
        return Err(StoreError::Invalid("principal must be positive".into()));
    }
    let percent = money_or_nothing(payload, "annual_rate")?;
    let rate = percent.over(&Money::parse("100").expect("money"));
    let years = whole_signed(payload, "years")?;
    if years < 1 {
        return Err(StoreError::Invalid("years must be at least one".into()));
    }
    let months = u32::try_from(years * 12)
        .map_err(|_| StoreError::Invalid("years is too long a term".into()))?;

    let rows = paydown_rows(&principal, &rate, months)?;
    let schedule: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "month": row.period,
                "payment": row.payment.to_f64_lossy(),
                "interest": row.interest_component.to_f64_lossy(),
                "principal": row.principal_component.to_f64_lossy(),
                "balance": row.balance.to_f64_lossy(),
            })
        })
        .collect();
    let interest = rows.iter().fold(Money::zero(), |total, row| {
        total.plus(&row.interest_component)
    });
    Ok(json!({
        "principal": principal.round(2).to_f64_lossy(),
        "annual_rate": percent.to_f64_lossy(),
        "years": years,
        "months": months,
        "monthly_payment": rows.first().map(|row| row.payment.to_f64_lossy()).unwrap_or(0.0),
        "total_interest": interest.round(2).to_f64_lossy(),
        "schedule": schedule,
    }))
}

/// The caller's loans for a company, oldest first.
fn mine<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Vec<Value>, StoreError> {
    let mut rows: Vec<Value> = Records::new(graph, DEBT_LOAN)
        .list(scope, 4000)?
        .into_iter()
        .filter(|row| field(row, "company_id") == company_id)
        .collect();
    rows.sort_by(|one, two| field(one, "created_at").cmp(field(two, "created_at")));
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn loan(
        id: &str,
        principal: &str,
        rate: &str,
        months: u32,
        balance: &str,
        created: &str,
    ) -> Value {
        json!({"id": id, "company_id": "c-1", "loan_name": id, "principal": principal,
               "interest_rate": rate, "term_months": months, "remaining_balance": balance,
               "created_at": created})
    }

    #[test]
    fn a_zero_rate_pays_the_principal_down_over_the_term() {
        let each =
            payment(&Money::parse("1200").expect("money"), &Money::zero(), 12).expect("payment");
        assert_eq!(each.to_fixed(6), "100.000000");
    }

    #[test]
    fn a_five_percent_loan_of_a_hundred_thousand_over_ten_years_pays_1060_66_a_month() {
        // the figure a desk checks first; the annuity formula lives here once, not per caller
        let each = payment(
            &Money::parse("100000.00").expect("money"),
            &Money::parse("0.05").expect("money"),
            120,
        )
        .expect("payment");
        assert_eq!(each.round(2).to_fixed(6), "1060.660000");
    }

    #[test]
    fn a_schedule_ends_at_nothing_owed_and_never_below_it() {
        let rows =
            paydown(&loan("l-1", "1200.00", "0", 12, "1200.00", "2026-03-01")).expect("schedule");
        assert_eq!(rows.len(), 12);
        assert_eq!(rows[0]["payment"], json!("100.000000"));
        assert_eq!(rows[0]["interest_component"], json!("0.000000"));
        assert_eq!(rows[11]["balance"], json!("0.000000"));
        for row in &rows {
            assert_ne!(
                row["balance"],
                json!("-0.000000"),
                "a loan never owes less than nothing"
            );
        }
    }

    #[test]
    fn a_schedule_charges_interest_on_what_is_still_owed() {
        let rows = paydown(&loan(
            "l-1",
            "12000.00",
            "0.12",
            12,
            "12000.00",
            "2026-03-01",
        ))
        .expect("schedule");
        let first = &rows[0];
        // one percent a month on twelve thousand
        assert_eq!(first["interest_component"], json!("120.000000"));
        // interest falls as the balance does, compared as amounts rather than as text
        let charged: Vec<Money> = rows
            .iter()
            .map(|row| {
                Money::parse(row["interest_component"].as_str().expect("text")).expect("money")
            })
            .collect();
        assert!(
            charged
                .windows(2)
                .all(|pair| pair[1].to_f64_lossy() < pair[0].to_f64_lossy()),
            "interest falls as the balance does"
        );
    }

    #[test]
    fn the_summary_weights_the_rate_by_balance_not_by_loan() {
        let graph = FakeGraph::returning(vec![
            loan("l-1", "10000.00", "0.10", 24, "20000.00", "2026-03-01"),
            loan("l-2", "10000.00", "0.20", 24, "0.00", "2026-03-02"),
        ]);
        let answer = summary(
            &graph,
            &scope(),
            "c-1",
            &Money::parse("50000").expect("equity"),
        )
        .expect("summary");
        assert_eq!(answer["total_debt"], json!("20000.000000"));
        // the cheaper loan is the one still owed, so the weighted rate is its rate alone
        assert_eq!(answer["weighted_avg_rate"], json!("0.100000"));
        assert_eq!(answer["debt_to_equity"], json!("0.400000"));
        assert_eq!(answer["loans"].as_array().expect("rows").len(), 2);
    }

    #[test]
    fn a_book_with_no_debt_has_no_ratio_rather_than_a_division_by_nothing() {
        let graph = FakeGraph::returning(vec![]);
        let answer = summary(&graph, &scope(), "c-1", &Money::zero()).expect("summary");
        assert_eq!(answer["total_debt"], json!("0.000000"));
        assert_eq!(answer["weighted_avg_rate"], json!("0.000000"));
        assert_eq!(answer["debt_to_equity"], json!("0.000000"));
    }

    #[test]
    fn a_schedule_for_a_loan_the_caller_does_not_hold_is_a_miss() {
        let graph = FakeGraph::returning(vec![loan(
            "l-1",
            "100.00",
            "0.10",
            12,
            "100.00",
            "2026-03-01",
        )]);
        let missing = schedule(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "loan_id": "l-9"}),
        );
        assert!(matches!(missing, Err(StoreError::NotFound)));
    }

    #[test]
    fn a_loan_without_a_term_is_refused_before_it_is_written() {
        let graph = FakeGraph::returning(vec![]);
        let refused = create_loan(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "loan_name": "revolving", "principal": "100.00",
                    "interest_rate": "0.05", "term_months": "0"}),
        );
        assert!(refused.is_err());
    }
}

#[cfg(test)]
mod quick_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_loan_pays_down_month_by_month_and_lands_on_nothing() {
        let answer = quick_amortisation(&json!({"principal": 1000, "annual_rate": 12, "years": 1}))
            .expect("a schedule");
        assert_eq!(answer["months"], json!(12));
        let schedule = answer["schedule"].as_array().expect("rows");
        assert_eq!(schedule.len(), 12);
        // the first month charges one percent on the whole balance, as 12% over twelve months must
        assert_eq!(schedule[0]["interest"], json!(10.0));
        assert_eq!(schedule[0]["balance"], json!(921.15));
        assert_eq!(answer["monthly_payment"], json!(88.85));
        // the schedule lands on nothing, so the loan is actually cleared
        assert_eq!(schedule[11]["balance"], json!(0.0));
        // and what is paid over the year is the principal plus the interest
        let principal: f64 = schedule
            .iter()
            .map(|row| row["principal"].as_f64().unwrap())
            .sum();
        let interest: f64 = schedule
            .iter()
            .map(|row| row["interest"].as_f64().unwrap())
            .sum();
        assert!((principal + interest - 88.85 * 12.0).abs() < 0.02);
        assert!((interest - answer["total_interest"].as_f64().unwrap()).abs() < 0.02);
    }

    #[test]
    fn a_loan_at_no_rate_is_the_principal_split_evenly() {
        let answer = quick_amortisation(&json!({"principal": 1200, "annual_rate": 0, "years": 1}))
            .expect("a schedule");
        assert_eq!(answer["monthly_payment"], json!(100.0));
        assert_eq!(answer["total_interest"], json!(0.0));
        let schedule = answer["schedule"].as_array().expect("rows");
        assert_eq!(schedule[5]["payment"], json!(100.0));
        assert_eq!(schedule[5]["balance"], json!(600.0));

        // a term of nothing, or a principal of nothing, is refused rather than divided by
        assert!(matches!(
            quick_amortisation(&json!({"principal": 1000, "annual_rate": 5, "years": 0})),
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            quick_amortisation(&json!({"principal": 0, "annual_rate": 5, "years": 1})),
            Err(StoreError::Invalid(_))
        ));
    }
}
