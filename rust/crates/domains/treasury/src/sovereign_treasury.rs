#![forbid(unsafe_code)]
//! sovereign-treasury-service: a country's accounts, the debt it carries, and its fiscal position.
//!
//! Three small books, each Book-scoped and exact. Shapes kept from the service: an account is answered with its
//! id, its type and its balance only, because that is what a treasury desk reads back; the account listing adds
//! the balances up and answers the country, the accounts and the total; debt is answered the same narrow way,
//! with the instrument and the outstanding amount; the debt listing adds the outstanding amounts up and counts
//! the instruments; and the fiscal position is one record per country that is overwritten rather than added to,
//! with the deficit derived as expenditure less revenue and the two ratios taken as given, because the service
//! never computed them - a caller who leaves them out gets zero, not an invention.
//!
//! Amounts are decimal strings in the answer, so a treasury that owes more than the digits of a float can hold
//! still reads back exactly what it owes.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, FISCAL_POSITION, SOVEREIGN_ACCOUNT, SOVEREIGN_DEBT,
};

use crate::helpers::{field, money, money_or_nothing, or_default, remember, text};

/// The account types the service recognises.
pub const ACCOUNT_TYPES: [&str; 4] = [
    "consolidated_revenue",
    "stabilization_fund",
    "debt_management",
    "foreign_reserves",
];

/// `POST /accounts`: register an account and answer with the three figures a desk reads.
pub fn create_account<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let account_type = or_default(payload, "account_type", "");
    if !ACCOUNT_TYPES.contains(&account_type.as_str()) {
        return Err(StoreError::Invalid(format!(
            "Invalid account type. Must be one of {ACCOUNT_TYPES:?}"
        )));
    }
    let balance = money(payload, "balance")?;

    let stored = remember(
        graph,
        scope,
        SOVEREIGN_ACCOUNT,
        json!({
            "country": text(payload, "country"),
            "account_type": account_type,
            "balance": wire(&balance),
            "currency": or_default(payload, "currency", "USD"),
            "description": text(payload, "description"),
            "created_at": text(payload, "created_at"),
        }),
    )?;

    // a desk reads back the id, the type and the balance, and nothing it did not ask for
    Ok(json!({
        "id": stored["id"],
        "type": stored["account_type"],
        "balance": stored["balance"],
    }))
}

/// `GET /accounts/{country}`: the country's accounts and what they hold.
pub fn list_accounts<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    country: &str,
) -> Result<Value, StoreError> {
    let mut mine: Vec<Value> = Records::new(graph, SOVEREIGN_ACCOUNT)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| field(row, "country") == country)
        .collect();
    mine.sort_by(|one, two| field(one, "created_at").cmp(field(two, "created_at")));

    let mut total = Money::zero();
    for row in &mine {
        total = total.plus(&money_or_nothing(row, "balance")?);
    }

    Ok(json!({
        "country": country,
        "accounts": mine,
        "total_balance": wire(&total),
    }))
}

/// `POST /debt`: register an instrument and answer with the amount outstanding.
pub fn register_debt<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let outstanding = money(payload, "outstanding")?;
    let stored = remember(
        graph,
        scope,
        SOVEREIGN_DEBT,
        json!({
            "country": text(payload, "country"),
            "instrument": text(payload, "instrument"),
            "principal": wire(&money(payload, "principal")?),
            "interest_rate": wire(&money_or_nothing(payload, "interest_rate")?),
            "maturity_date": text(payload, "maturity_date"),
            "outstanding": wire(&outstanding),
            "currency": or_default(payload, "currency", "USD"),
            "created_at": text(payload, "created_at"),
        }),
    )?;

    Ok(json!({
        "id": stored["id"],
        "instrument": stored["instrument"],
        "outstanding": stored["outstanding"],
    }))
}

/// `GET /debt/{country}`: what the country owes, by instrument and in total.
pub fn list_debts<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    country: &str,
) -> Result<Value, StoreError> {
    let mut mine: Vec<Value> = Records::new(graph, SOVEREIGN_DEBT)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| field(row, "country") == country)
        .collect();
    mine.sort_by(|one, two| field(one, "created_at").cmp(field(two, "created_at")));

    let mut total = Money::zero();
    for row in &mine {
        total = total.plus(&money_or_nothing(row, "outstanding")?);
    }

    Ok(json!({
        "country": country,
        "debts": mine,
        "total_debt": wire(&total),
        "instruments": mine.len(),
    }))
}

/// `POST /fiscal-position`: one position per country, overwritten rather than added to.
pub fn set_fiscal_position<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let country = text(payload, "country");
    let revenue = money(payload, "total_revenue")?;
    let expenditure = money(payload, "total_expenditure")?;
    // the deficit is derived, not trusted: expenditure over revenue
    let deficit = expenditure.minus(&revenue);

    let body = json!({
        "country": country,
        "fiscal_year": text(payload, "fiscal_year"),
        "total_revenue": wire(&revenue),
        "total_expenditure": wire(&expenditure),
        "fiscal_deficit": wire(&deficit),
        "deficit_to_gdp": wire(&money_or_nothing(payload, "deficit_to_gdp")?),
        "total_debt": wire(&money_or_nothing(payload, "total_debt")?),
        "debt_to_gdp": wire(&money_or_nothing(payload, "debt_to_gdp")?),
        "foreign_reserves": wire(&money_or_nothing(payload, "foreign_reserves")?),
        "months_import_cover": wire(&money_or_nothing(payload, "months_import_cover")?),
        "created_at": text(payload, "created_at"),
    });

    let stored = Records::new(graph, FISCAL_POSITION)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "country") == country);

    match stored {
        Some(row) => {
            let id = field(&row, "id").to_string();
            Records::new(graph, FISCAL_POSITION).update(scope, &id, &body)
        }
        None => remember(graph, scope, FISCAL_POSITION, body),
    }
}

/// `GET /fiscal-position/{country}`: the position as it stands, or a miss when none was set.
pub fn get_fiscal_position<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    country: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, FISCAL_POSITION)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "country") == country)
        .ok_or(StoreError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn an_account_is_answered_with_its_id_type_and_balance_only() {
        let graph = FakeGraph::returning(vec![json!({"id": "sa-1"})]);
        let answer = create_account(
            &graph,
            &scope(),
            &json!({"country": "ZW", "account_type": "foreign_reserves",
                    "balance": "123456789012345678901.55", "description": "reserves"}),
        )
        .expect("account");

        assert_eq!(answer["id"], json!("sa-1"));
        assert_eq!(answer["type"], json!("foreign_reserves"));
        assert_eq!(answer["balance"], json!("123456789012345678901.550000"));
        assert!(
            answer.get("description").is_none(),
            "the desk asked for three figures"
        );

        let (_, params) = graph.last();
        assert_eq!(params["props"]["country"], json!("ZW"));
    }

    #[test]
    fn an_unknown_account_type_is_refused_before_anything_is_written() {
        let graph = FakeGraph::returning(vec![]);
        let refused = create_account(
            &graph,
            &scope(),
            &json!({"country": "ZW", "account_type": "chequing", "balance": "1.00"}),
        );
        assert!(matches!(refused, Err(StoreError::Invalid(_))));
    }

    #[test]
    fn the_account_listing_totals_the_country_s_balances_exactly() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "sa-1", "country": "ZW", "account_type": "foreign_reserves",
                   "balance": "100000000000000000001.00", "created_at": "2026-03-01"}),
            json!({"id": "sa-2", "country": "ZW", "account_type": "stabilization_fund",
                   "balance": "200000000000000000000.50", "created_at": "2026-03-02"}),
            json!({"id": "sa-3", "country": "ZM", "account_type": "foreign_reserves",
                   "balance": "5.00", "created_at": "2026-03-03"}),
        ]);
        let answer = list_accounts(&graph, &scope(), "ZW").expect("accounts");
        assert_eq!(answer["accounts"].as_array().expect("rows").len(), 2);
        assert_eq!(
            answer["total_balance"],
            json!("300000000000000000001.500000")
        );
    }

    #[test]
    fn a_country_with_no_accounts_totals_nothing() {
        let graph = FakeGraph::returning(vec![]);
        let answer = list_accounts(&graph, &scope(), "ZW").expect("accounts");
        assert_eq!(answer["total_balance"], json!("0.000000"));
        assert_eq!(answer["accounts"].as_array().expect("rows").len(), 0);
    }

    #[test]
    fn debt_is_answered_narrowly_and_listed_with_its_total() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "d-1"})],
            vec![
                json!({"id": "d-1", "country": "ZW", "instrument": "eurobond",
                       "outstanding": "5000000.00", "created_at": "2026-03-01"}),
                json!({"id": "d-2", "country": "ZW", "instrument": "treasury_bill",
                       "outstanding": "2500000.00", "created_at": "2026-03-02"}),
            ],
        ]);
        let registered = register_debt(
            &graph,
            &scope(),
            &json!({"country": "ZW", "instrument": "eurobond", "principal": "6000000.00",
                    "interest_rate": "0.0875", "maturity_date": "2036-03-01",
                    "outstanding": "5000000.00"}),
        )
        .expect("debt");
        assert_eq!(registered["instrument"], json!("eurobond"));
        assert_eq!(registered["outstanding"], json!("5000000.000000"));
        assert!(registered.get("principal").is_none());

        let listed = list_debts(&graph, &scope(), "ZW").expect("debts");
        assert_eq!(listed["total_debt"], json!("7500000.000000"));
        assert_eq!(listed["instruments"], json!(2));
    }

    #[test]
    fn the_fiscal_deficit_is_derived_and_the_position_is_overwritten() {
        let graph = FakeGraph::queue(vec![
            vec![],
            vec![json!({"id": "fp-1"})],
            vec![json!({"id": "fp-1", "country": "ZW", "fiscal_year": "2026"})],
            vec![json!({"id": "fp-1"})],
        ]);
        let created = set_fiscal_position(
            &graph,
            &scope(),
            &json!({"country": "ZW", "fiscal_year": "2026", "total_revenue": "1000.00",
                    "total_expenditure": "1250.00"}),
        )
        .expect("position");
        assert_eq!(created["fiscal_deficit"], json!("250.000000"));
        // a ratio nobody supplied is zero, not an invention
        assert_eq!(created["deficit_to_gdp"], json!("0.000000"));

        let updated = set_fiscal_position(
            &graph,
            &scope(),
            &json!({"country": "ZW", "fiscal_year": "2026", "total_revenue": "2000.00",
                    "total_expenditure": "1900.00", "deficit_to_gdp": "-1.50"}),
        )
        .expect("position");
        let (statement, params) = graph.last();
        // the update answers the stored record; the figures are in the patch that was sent
        assert_eq!(updated, json!("fp-1"), "answer after {statement}");
        // a surplus is a negative deficit, which the arithmetic gives without a special case
        assert_eq!(params["patch"]["fiscal_deficit"], json!("-100.000000"));
        assert!(statement.contains("SET x += $patch"), "{statement}");
        assert_eq!(params["id"], json!("fp-1"));
        assert_eq!(params["patch"]["fiscal_deficit"], json!("-100.000000"));
    }

    #[test]
    fn a_country_with_no_fiscal_position_is_a_miss() {
        let graph = FakeGraph::returning(vec![]);
        assert!(matches!(
            get_fiscal_position(&graph, &scope(), "ZW"),
            Err(StoreError::NotFound)
        ));
    }
}
