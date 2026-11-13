#![forbid(unsafe_code)]
//! personal-finance-service, the rest of it: tax bands and what a year's income leaves, the savings goals, the income
//! sources, and the one-page view over all of it.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a person with no bands of their own is taxed on the four the service ships: nothing to 10,000, 15% to 30,000,
//!   25% to 70,000 and 35% above;
//! * bands must ascend by upper limit, be ordered by rate, and end with a top band that has no upper limit, since
//!   without one the highest slice of income would not be taxed at all;
//! * tax is charged band by band on the taxable amount, and the marginal rate is the rate of the band the last slice
//!   fell in;
//! * the effective rate is the tax over the gross income, and nothing when there is no income rather than dividing;
//! * the withholding is the year's tax over twelve, and the balance due is that tax less what was already paid;
//! * a goal's progress is its own current over target, never more than the whole, and the average across the goals is
//!   nothing when there are none;
//! * the monthly income is the sum of the sources as recorded, without converting a frequency;
//! * the debt-to-income ratio is the total debt over a year of that income, as a percentage.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{decimal, field, money, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, FINANCIAL_GOAL, INCOME_SOURCE, LEGACY_DEBT_ITEM,
    PERSONAL_DEBT, PERSONAL_TAX_BRACKET,
};

use crate::personal_finance::debt_states;

/// The four bands the service ships, used until a person sets their own.
const SHIPPED_BANDS: &[(&str, &str)] = &[
    ("10000", "0"),
    ("30000", "0.15"),
    ("70000", "0.25"),
    ("", "0.35"),
];

/// An amount that may simply be left out, which then reads as nothing.
fn optional_amount(payload: &Value, key: &str) -> Result<Money, StoreError> {
    Money::parse(&decimal(payload, key, "0"))
        .map_err(|_| refused(&format!("{key} must be an amount")))
}

fn hundred() -> Money {
    Money::parse("100").unwrap_or_else(|_| Money::zero())
}

/// Where one band ends: `None` for the top band.
fn band_top(band: &Value) -> Option<Money> {
    let up_to = band.get("up_to")?;
    if up_to.is_null() {
        return None;
    }
    Money::parse(up_to.to_string().trim_matches('"')).ok()
}

fn band_rate(band: &Value) -> Money {
    Money::parse(&decimal(band, "rate", "0")).unwrap_or_else(|_| Money::zero())
}

fn shipped_bands() -> Vec<Value> {
    SHIPPED_BANDS
        .iter()
        .map(|(up_to, rate)| {
            if up_to.is_empty() {
                json!({"up_to": Value::Null, "rate": rate})
            } else {
                json!({"up_to": up_to, "rate": rate})
            }
        })
        .collect()
}

/// The bands this person is taxed on, theirs if they have set any.
fn bands_of(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    let stored = Records::new(graph, PERSONAL_TAX_BRACKET).list(scope, 200)?;
    if stored.is_empty() {
        return Ok(shipped_bands());
    }
    let mut bands = stored;
    bands.sort_by(|one, two| match (band_top(one), band_top(two)) {
        (Some(left), Some(right)) => left.as_decimal().cmp(right.as_decimal()),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    Ok(bands)
}

/// `GET /tax/brackets`.
pub fn list_bands(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    Ok(json!({ "brackets": bands_of(graph, scope)? }))
}

/// `PUT /tax/brackets`: the bands are replaced whole, since a half-replaced set of bands taxes nonsense.
pub fn replace_bands(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let bands = payload
        .get("brackets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if bands.is_empty() {
        return Err(refused("brackets cannot be empty"));
    }

    let rates: Vec<String> = bands
        .iter()
        .map(|band| band_rate(band).to_fixed(6))
        .collect();
    let mut sorted_rates = rates.clone();
    sorted_rates.sort();
    if rates != sorted_rates {
        return Err(refused("brackets must be ordered by rate"));
    }

    let tops: Vec<Money> = bands.iter().filter_map(band_top).collect();
    let mut sorted_tops = tops.clone();
    sorted_tops.sort_by(|left, right| left.as_decimal().cmp(right.as_decimal()));
    if tops != sorted_tops {
        return Err(refused("band upper limits must ascend"));
    }
    if band_top(bands.last().unwrap_or(&Value::Null)).is_some() {
        return Err(refused("final band must be the top band (up_to null)"));
    }

    let records = Records::new(graph, PERSONAL_TAX_BRACKET);
    for existing in records.list(scope, 200)? {
        records.delete(scope, field(&existing, "id"))?;
    }
    for band in &bands {
        let up_to = band_top(band).map(|top| wire(&top));
        remember(
            graph,
            scope,
            PERSONAL_TAX_BRACKET,
            json!({"up_to": up_to, "rate": wire(&band_rate(band))}),
        )?;
    }

    Ok(json!({ "brackets": bands_of(graph, scope)? }))
}

/// The tax on a taxable amount, and the rate of the band the last slice fell in.
fn tax_on(taxable: &Money, bands: &[Value]) -> (Money, Money) {
    let mut tax = Money::zero();
    let mut lower = Money::zero();
    let mut marginal = Money::zero();

    for band in bands {
        let rate = band_rate(band);
        if taxable.as_decimal() > lower.as_decimal() {
            let top = match band_top(band) {
                Some(limit) => {
                    if taxable.as_decimal() < limit.as_decimal() {
                        taxable.clone()
                    } else {
                        limit
                    }
                }
                None => taxable.clone(),
            };
            tax = tax.plus(&top.minus(&lower).times(&rate));
            marginal = rate;
        }
        match band_top(band) {
            Some(limit) => {
                if taxable.as_decimal() <= limit.as_decimal() {
                    break;
                }
                lower = limit;
            }
            None => break,
        }
    }

    (tax, marginal)
}

/// `POST /tax/estimate`.
pub fn estimate_tax(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let annual = money(payload, "annual_income")?;
    if annual.is_negative() {
        return Err(refused("annual_income cannot be negative"));
    }
    let other = optional_amount(payload, "other_income")?;
    let deductions = optional_amount(payload, "deductions")?;
    let paid = optional_amount(payload, "paye_paid")?;

    let gross = annual.plus(&other);
    let mut taxable = gross.minus(&deductions);
    if taxable.is_negative() {
        taxable = Money::zero();
    }

    let bands = bands_of(graph, scope)?;
    let (tax, marginal) = tax_on(&taxable, &bands);
    let effective = if gross.is_zero() || gross.is_negative() {
        Money::zero()
    } else {
        tax.over(&gross)
    };
    let twelve = Money::from_i64(12);

    Ok(json!({
        "gross_income": wire(&gross.round(2)),
        "taxable_income": wire(&taxable.round(2)),
        "estimated_tax": wire(&tax.round(2)),
        "effective_rate": wire(&effective.round(4)),
        "marginal_rate": wire(&marginal),
        "net_after_tax": wire(&gross.minus(&tax).round(2)),
        "monthly_withholding": wire(&tax.over(&twelve).round(2)),
        "balance_due": wire(&tax.minus(&paid).round(2)),
    }))
}

/// `POST /goals`.
pub fn create_goal(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let target = money(payload, "target_amount")?;
    if target.is_negative() || target.is_zero() {
        return Err(refused("target_amount must be greater than zero"));
    }

    remember(
        graph,
        scope,
        FINANCIAL_GOAL,
        json!({
            "name": name,
            "target_amount": wire(&target),
            "current_amount": wire(&optional_amount(payload, "current_amount")?),
            "target_date": text(payload, "target_date"),
            "category": if text(payload, "category").is_empty() { "savings".to_string() } else { text(payload, "category") },
            "priority": field(payload, "priority").parse::<i64>().unwrap_or(3),
        }),
    )
}

/// `GET /goals`.
pub fn list_goals(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let goals = Records::new(graph, FINANCIAL_GOAL).list(scope, 2000)?;
    let mut progress = Money::zero();
    for goal in &goals {
        let current = Money::parse(field(goal, "current_amount")).unwrap_or_else(|_| Money::zero());
        let target = Money::parse(field(goal, "target_amount")).unwrap_or_else(|_| Money::zero());
        if target.is_negative() || target.is_zero() {
            continue;
        }
        progress = progress.plus(&current.over(&target));
    }
    let average = if goals.is_empty() {
        Money::zero()
    } else {
        progress.over(&Money::from_i64(goals.len() as i64))
    };

    Ok(json!({
        "goals": goals,
        "total": goals.len(),
        "progress_avg": wire(&average),
    }))
}

/// `POST /income`.
pub fn add_income(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let source = text(payload, "source");
    if source.is_empty() {
        return Err(refused("source is required"));
    }
    remember(
        graph,
        scope,
        INCOME_SOURCE,
        json!({
            "source": source,
            "amount": wire(&optional_amount(payload, "amount")?),
            "frequency": if text(payload, "frequency").is_empty() { "monthly".to_string() } else { text(payload, "frequency") },
        }),
    )
}

/// `GET /income`.
pub fn list_income(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let sources = Records::new(graph, INCOME_SOURCE).list(scope, 2000)?;
    let mut total = Money::zero();
    for source in &sources {
        total =
            total.plus(&Money::parse(field(source, "amount")).unwrap_or_else(|_| Money::zero()));
    }
    Ok(json!({
        "sources": sources,
        "total_monthly": wire(&total),
    }))
}

/// `POST /debts` in its older shape: a creditor, a balance, a rate and a minimum payment, kept apart from the
/// amortising ledger so a figure typed in from an old list is never replayed as if it were a payment history.
pub fn create_legacy_debt(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let creditor = text(payload, "creditor");
    if creditor.is_empty() {
        return Err(refused("creditor is required"));
    }
    remember(
        graph,
        scope,
        LEGACY_DEBT_ITEM,
        json!({
            "creditor": creditor,
            "balance": wire(&optional_amount(payload, "balance")?),
            "interest_rate": wire(&optional_amount(payload, "interest_rate")?),
            "min_payment": wire(&optional_amount(payload, "min_payment")?),
            "type": if text(payload, "type").is_empty() { "credit_card".to_string() } else { text(payload, "type") },
        }),
    )
}

/// `GET /overview`: the one page over income, debt and goals.
pub fn overview(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let today = text(query, "today");
    let mut income = Money::zero();
    for source in Records::new(graph, INCOME_SOURCE).list(scope, 2000)? {
        income =
            income.plus(&Money::parse(field(&source, "amount")).unwrap_or_else(|_| Money::zero()));
    }

    let mut debt = Money::zero();
    for item in Records::new(graph, LEGACY_DEBT_ITEM).list(scope, 2000)? {
        debt = debt.plus(&Money::parse(field(&item, "balance")).unwrap_or_else(|_| Money::zero()));
    }
    for state in debt_states(graph, scope, &today)? {
        debt = debt.plus(&Money::parse(field(&state, "balance")).unwrap_or_else(|_| Money::zero()));
    }

    let goals = Records::new(graph, FINANCIAL_GOAL).list(scope, 2000)?;
    let mut current = Money::zero();
    let mut target = Money::zero();
    for goal in &goals {
        current = current
            .plus(&Money::parse(field(goal, "current_amount")).unwrap_or_else(|_| Money::zero()));
        target = target
            .plus(&Money::parse(field(goal, "target_amount")).unwrap_or_else(|_| Money::zero()));
    }

    // a year of that income, and one rather than nothing when there is none, which is what the original divided by
    let mut year_income = income.times(&Money::from_i64(12));
    if year_income.as_decimal()
        < Money::parse("1")
            .unwrap_or_else(|_| Money::zero())
            .as_decimal()
    {
        year_income = Money::parse("1").unwrap_or_else(|_| Money::zero());
    }
    let ratio = debt.over(&year_income).times(&hundred());

    Ok(json!({
        "monthly_income": wire(&income),
        "total_debt": wire(&debt.round(2)),
        "debt_to_income": wire(&ratio),
        "active_goals": goals.len(),
        "goals_progress": wire(&current),
        "goals_target": wire(&target),
    }))
}

/// Unused today: the debt reader used by `overview` lives beside the ledger that computes it.
#[allow(dead_code)]
fn personal_debt_kind() -> vimbai_store::RecordKind {
    PERSONAL_DEBT
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn the_shipped_bands_tax_a_year_of_income_band_by_band() {
        let graph = FakeGraph::returning(vec![]);
        // 50,000 taxable: nothing to 10,000, 15% of 20,000, 25% of 20,000
        let answer =
            estimate_tax(&graph, &scope(), &json!({"annual_income": "50000"})).expect("estimated");
        assert_eq!(answer["estimated_tax"], json!("8000.000000"));
        assert_eq!(answer["marginal_rate"], json!("0.250000"));
        assert_eq!(answer["effective_rate"], json!("0.160000"));
        assert_eq!(answer["monthly_withholding"], json!("666.670000"));
    }

    #[test]
    fn nothing_is_taxed_below_the_first_band() {
        let graph = FakeGraph::returning(vec![]);
        let answer =
            estimate_tax(&graph, &scope(), &json!({"annual_income": "9000"})).expect("estimated");
        assert_eq!(answer["estimated_tax"], json!("0.000000"));
        assert_eq!(answer["net_after_tax"], json!("9000.000000"));
    }

    #[test]
    fn a_deduction_reduces_what_is_taxed_but_not_the_gross() {
        let graph = FakeGraph::returning(vec![]);
        let answer = estimate_tax(
            &graph,
            &scope(),
            &json!({"annual_income": "50000", "deductions": "10000", "paye_paid": "5000"}),
        )
        .expect("estimated");
        assert_eq!(answer["gross_income"], json!("50000.000000"));
        assert_eq!(answer["taxable_income"], json!("40000.000000"));
        // 15% of 20,000 plus 25% of 10,000
        assert_eq!(answer["estimated_tax"], json!("5500.000000"));
        assert_eq!(answer["balance_due"], json!("500.000000"));
    }

    #[test]
    fn a_year_with_no_income_has_no_effective_rate_rather_than_a_division() {
        let graph = FakeGraph::returning(vec![]);
        let answer =
            estimate_tax(&graph, &scope(), &json!({"annual_income": "0"})).expect("estimated");
        assert_eq!(answer["effective_rate"], json!("0.000000"));
        assert_eq!(answer["estimated_tax"], json!("0.000000"));
    }

    #[test]
    fn bands_that_do_not_ascend_or_do_not_end_in_a_top_band_are_refused() {
        let graph = FakeGraph::returning(vec![]);
        assert!(replace_bands(
            &graph,
            &scope(),
            &json!({"brackets": [{"up_to": 20000, "rate": 0.2}, {"up_to": 10000, "rate": 0.1}]}),
        )
        .is_err());
        assert!(replace_bands(
            &graph,
            &scope(),
            &json!({"brackets": [{"up_to": 20000, "rate": 0.2}, {"up_to": 40000, "rate": 0.3}]}),
        )
        .is_err());
        assert!(replace_bands(&graph, &scope(), &json!({"brackets": []})).is_err());
    }

    #[test]
    fn bands_a_person_sets_are_the_ones_their_tax_is_charged_on() {
        // three bands replacing the shipped four, then an estimate through the same reader
        let stored = vec![
            json!({"id": "b-1", "up_to": "12000.000000", "rate": "0.000000"}),
            json!({"id": "b-2", "up_to": null, "rate": "0.400000"}),
        ];
        let graph = FakeGraph::returning(stored);
        let answer =
            estimate_tax(&graph, &scope(), &json!({"annual_income": "22000"})).expect("estimated");
        // 10,000 above the free band at 40%
        assert_eq!(answer["estimated_tax"], json!("4000.000000"));
        assert_eq!(answer["marginal_rate"], json!("0.400000"));
    }

    #[test]
    fn a_goal_reports_its_own_progress_and_the_average_across_goals() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "g-1", "current_amount": "500.000000", "target_amount": "1000.000000"}),
            json!({"id": "g-2", "current_amount": "250.000000", "target_amount": "1000.000000"}),
        ]);
        let answer = list_goals(&graph, &scope()).expect("listed");
        assert_eq!(answer["total"], json!(2));
        // 0.5 and 0.25 average to 0.375
        assert_eq!(answer["progress_avg"], json!("0.375000"));
    }

    #[test]
    fn a_goal_needs_a_target_to_be_worth_keeping() {
        let graph = FakeGraph::returning(vec![]);
        assert!(create_goal(&graph, &scope(), &json!({"name": "Bike"})).is_err());
    }

    #[test]
    fn the_monthly_income_is_the_sum_of_the_sources_as_recorded() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "s-1", "source": "Salary", "amount": "1200.000000"}),
            json!({"id": "s-2", "source": "Rent", "amount": "300.500000"}),
        ]);
        let answer = list_income(&graph, &scope()).expect("listed");
        assert_eq!(answer["total_monthly"], json!("1500.500000"));
    }

    #[test]
    fn the_overview_puts_income_debt_and_goals_on_one_page() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "s-1", "amount": "1000.000000"})],
            vec![json!({"id": "l-1", "balance": "2000.000000"})],
            vec![],
            vec![
                json!({"id": "g-1", "current_amount": "100.000000", "target_amount": "400.000000"}),
            ],
        ]);
        let answer = overview(&graph, &scope(), &json!({"today": "2026-01-01"})).expect("seen");
        assert_eq!(answer["monthly_income"], json!("1000.000000"));
        assert_eq!(answer["total_debt"], json!("2000.000000"));
        // 2000 over a year of 12,000 is 16.67%
        assert_eq!(answer["debt_to_income"], json!("16.666667"));
        assert_eq!(answer["active_goals"], json!(1));
        assert_eq!(answer["goals_target"], json!("400.000000"));
    }

    #[test]
    fn an_old_style_debt_needs_a_creditor() {
        let graph = FakeGraph::returning(vec![]);
        assert!(create_legacy_debt(&graph, &scope(), &json!({"balance": "500"})).is_err());
    }
}
