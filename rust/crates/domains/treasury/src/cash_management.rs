#![forbid(unsafe_code)]
//! cash-management-service: the accounts money sits in, transfers between them, and the liquidity those
//! balances add up to.
//!
//! Balances, minimums, transfers and ratios are exact. A transfer can only move money between the caller's
//! own Book-visible accounts, and only if the source keeps its minimum, which is the check the service
//! made before it touched anything.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * an account type outside operating, reserve and investment is refused with the list of valid ones;
//! * a transfer is recorded as completed, not pending, and both balances move before anything answers;
//! * a transfer from an account to itself passes both checks and nets to nothing;
//! * liquidity is the operating, reserve and investment balances added together, and the ratio is cash over
//!   short-term obligations rounded to two places, or nothing at all when there are no obligations;
//! * the liquidity ratio is published as a plain figure rather than a string, because it is a ratio and not
//!   an amount of money.
//!
//! One thing deliberately shared: both this service and cash-optimization-service wrote `:CashAccount`
//! nodes, each under its own edge, so neither could see the accounts the other made. The port keeps one
//! `:CashAccount` under one edge, so the accounts these two services describe are one set. A cash-management
//! account carries the bank, its number, and a minimum; a cash-optimization account carries a company and
//! an interest rate; a record may carry either set of properties or both.

use crate::helpers::{field, money, money_or_nothing, or_default, refused, remember, text};
use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, CASH_ACCOUNT, CASH_TRANSFER, LIQUIDITY_POSITION,
};

/// `POST /accounts`.
pub fn create_account(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let account_type = or_default(payload, "type", "operating");
    if !["operating", "reserve", "investment"].contains(&account_type.as_str()) {
        return Err(refused(
            "Invalid account type. Must be one of ['operating', 'reserve', 'investment']",
        ));
    }
    let balance = money_or_nothing(payload, "balance")?;
    let minimum = money_or_nothing(payload, "min_balance")?;

    remember(
        graph,
        scope,
        CASH_ACCOUNT,
        json!({
            "account_name": text(payload, "account_name"),
            "bank": text(payload, "bank"),
            "account_number": text(payload, "account_number"),
            "currency": or_default(payload, "currency", "USD"),
            "balance": wire(&balance),
            "min_balance": wire(&minimum),
            "type": account_type,
            "status": "active",
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /accounts`.
pub fn list_accounts(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = query
        .get("type")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut all: Vec<Value> = Records::new(graph, CASH_ACCOUNT)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            wanted
                .as_deref()
                .map(|kind| field(row, "type") == kind)
                .unwrap_or(true)
        })
        .collect();
    all.sort_by(|one, two| field(one, "account_name").cmp(field(two, "account_name")));
    Ok(Value::Array(all))
}

/// `POST /transfers`: money between two of the caller's own accounts.
pub fn create_transfer(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let from_id = text(payload, "from_account_id");
    let to_id = text(payload, "to_account_id");
    let amount_moved = money(payload, "amount")?;

    let accounts = Records::new(graph, CASH_ACCOUNT).list(scope, 2000)?;
    let find = |id: &str| accounts.iter().find(|row| field(row, "id") == id).cloned();
    let (Some(from), Some(to)) = (find(&from_id), find(&to_id)) else {
        return Err(StoreError::NotFound);
    };

    let from_balance = money_or_nothing(&from, "balance")?;
    let from_minimum = money_or_nothing(&from, "min_balance")?;
    if from_balance.minus(&amount_moved) < from_minimum {
        return Err(refused("Transfer would breach minimum balance"));
    }

    let to_balance = money_or_nothing(&to, "balance")?;
    Records::new(graph, CASH_ACCOUNT).update(
        scope,
        &from_id,
        &json!({"balance": wire(&from_balance.minus(&amount_moved))}),
    )?;
    Records::new(graph, CASH_ACCOUNT).update(
        scope,
        &to_id,
        &json!({"balance": wire(&to_balance.plus(&amount_moved))}),
    )?;

    remember(
        graph,
        scope,
        CASH_TRANSFER,
        json!({
            "from_account_id": from_id,
            "to_account_id": to_id,
            "amount": wire(&amount_moved),
            "currency": or_default(payload, "currency", "USD"),
            "transfer_date": text(payload, "transfer_date"),
            "status": "completed",
            "reference": text(payload, "reference"),
            "notes": text(payload, "notes"),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /transfers`.
pub fn list_transfers(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let limit = vimbai_store::query::query_page(query, 50, 2000);
    let mut all = Records::new(graph, CASH_TRANSFER).list(scope, 2000)?;
    all.sort_by(|one, two| field(two, "transfer_date").cmp(field(one, "transfer_date")));
    all.truncate(limit);
    Ok(Value::Array(all))
}

/// Sorts one balance into the operating, reserve or investment pot, whichever name the account used.
fn account_by_kind(
    kind: &str,
    balance: &Money,
    operating: &mut Money,
    reserve: &mut Money,
    invested: &mut Money,
) {
    match kind {
        "operating" => *operating = operating.plus(balance),
        "reserve" => *reserve = reserve.plus(balance),
        "investment" => *invested = invested.plus(balance),
        _ => {}
    }
}

/// `POST /liquidity`: what the caller's balances add up to, and how far they cover what is owed soon.
pub fn calculate_liquidity(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let obligations = money_or_nothing(payload, "short_term_obligations")?;
    let mut operating = Money::zero();
    let mut reserve = Money::zero();
    let mut invested = Money::zero();

    for account in Records::new(graph, CASH_ACCOUNT).list(scope, 2000)? {
        let balance = money_or_nothing(&account, "balance")?;
        match field(&account, "type") {
            "operating" => operating = operating.plus(&balance),
            "reserve" => reserve = reserve.plus(&balance),
            "investment" => invested = invested.plus(&balance),
            // an account of the shape cash-optimization writes carries account_type instead of type;
            // the two labels are one set of accounts, so both names are read
            _ => account_by_kind(
                field(&account, "account_type"),
                &balance,
                &mut operating,
                &mut reserve,
                &mut invested,
            ),
        }
    }

    let total = operating.plus(&reserve).plus(&invested);
    // a ratio, not money: published as a plain figure, rounded to two places, nothing when nothing is owed
    let ratio = if obligations > Money::zero() {
        total.over(&obligations).round(2).to_fixed(2)
    } else {
        "0.00".to_string()
    };

    remember(
        graph,
        scope,
        LIQUIDITY_POSITION,
        json!({
            "position_date": text(payload, "position_date"),
            "total_cash": wire(&total),
            "operating_cash": wire(&operating),
            "reserve_cash": wire(&reserve),
            "invested_cash": wire(&invested),
            "short_term_obligations": wire(&obligations),
            "liquidity_ratio": ratio,
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /liquidity`.
pub fn list_liquidity_positions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let limit = vimbai_store::query::query_page(query, 30, 2000);
    let mut all = Records::new(graph, LIQUIDITY_POSITION).list(scope, 2000)?;
    all.sort_by(|one, two| field(two, "position_date").cmp(field(one, "position_date")));
    all.truncate(limit);
    Ok(Value::Array(all))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn account(id: &str, kind: &str, balance: &str, minimum: &str) -> Value {
        json!({"id": id, "account_name": format!("acct {id}"), "type": kind,
               "balance": balance, "min_balance": minimum})
    }

    #[test]
    fn an_account_type_the_service_does_not_know_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        let refused = create_account(
            &graph,
            &scope(),
            &json!({"account_name": "Petty", "bank": "CBZ", "account_number": "1", "type": "petty"}),
        );
        assert!(refused.is_err());
    }

    #[test]
    fn a_transfer_moves_both_balances_and_is_recorded_completed() {
        let graph = FakeGraph::queue(vec![
            vec![
                account("a-1", "operating", "1000.000000", "200.000000"),
                account("a-2", "reserve", "50.000000", "0.000000"),
            ],
            vec![json!({"id": "a-1"})],
            vec![json!({"id": "a-2"})],
            vec![json!({"id": "t-1"})],
        ]);
        let answer = create_transfer(
            &graph,
            &scope(),
            &json!({"from_account_id": "a-1", "to_account_id": "a-2", "amount": "300.00",
                    "transfer_date": "2026-03-15"}),
        )
        .expect("transfer");
        assert_eq!(answer["status"], json!("completed"));
        assert_eq!(answer["amount"], json!("300.000000"));
        // the source is set to 700 and the destination to 350
        let (_, out) = graph.statement(1);
        assert_eq!(out["patch"]["balance"], json!("700.000000"));
        let (_, into) = graph.statement(2);
        assert_eq!(into["patch"]["balance"], json!("350.000000"));
    }

    #[test]
    fn a_transfer_that_would_breach_the_minimum_is_refused_before_anything_moves() {
        let graph = FakeGraph::queue(vec![vec![
            account("a-1", "operating", "300.000000", "200.000000"),
            account("a-2", "reserve", "0.000000", "0.000000"),
        ]]);
        let refused = create_transfer(
            &graph,
            &scope(),
            &json!({"from_account_id": "a-1", "to_account_id": "a-2", "amount": "150.00"}),
        );
        assert!(refused.is_err());
        // only the read ran: nothing was written
        assert_eq!(graph.statements(), 1);
    }

    #[test]
    fn a_transfer_to_an_account_the_caller_does_not_hold_is_not_found() {
        let graph =
            FakeGraph::returning(vec![account("a-1", "operating", "1000.000000", "0.000000")]);
        let missing = create_transfer(
            &graph,
            &scope(),
            &json!({"from_account_id": "a-1", "to_account_id": "someone-else", "amount": "1.00"}),
        );
        assert!(matches!(missing, Err(StoreError::NotFound)));
    }

    #[test]
    fn liquidity_adds_each_kind_and_rounds_its_ratio_to_two_places() {
        let graph = FakeGraph::queue(vec![
            vec![
                account("a-1", "operating", "600.000000", "0.000000"),
                account("a-2", "reserve", "300.000000", "0.000000"),
                account("a-3", "investment", "100.000000", "0.000000"),
            ],
            vec![json!({"id": "p-1"})],
        ]);
        let answer = calculate_liquidity(
            &graph,
            &scope(),
            &json!({"short_term_obligations": "700.00", "position_date": "2026-03-15"}),
        )
        .expect("liquidity");
        assert_eq!(answer["total_cash"], json!("1000.000000"));
        assert_eq!(answer["operating_cash"], json!("600.000000"));
        // 1000 / 700 = 1.42857...
        assert_eq!(answer["liquidity_ratio"], json!("1.43"));
    }

    #[test]
    fn liquidity_with_nothing_owed_reports_no_ratio_rather_than_dividing() {
        let graph = FakeGraph::queue(vec![
            vec![account("a-1", "operating", "600.000000", "0.000000")],
            vec![json!({"id": "p-1"})],
        ]);
        let answer = calculate_liquidity(&graph, &scope(), &json!({})).expect("liquidity");
        assert_eq!(answer["liquidity_ratio"], json!("0.00"));
        assert_eq!(answer["short_term_obligations"], json!("0.000000"));
    }

    #[test]
    fn an_account_written_by_cash_optimisation_counts_in_liquidity() {
        let graph = FakeGraph::queue(vec![
            vec![
                account("a-1", "operating", "100.000000", "0.000000"),
                // the other service's shape: account_type, no type and no minimum
                json!({"id": "a-2", "account_name": "Opt", "account_type": "reserve",
                       "balance": "250.000000"}),
            ],
            vec![json!({"id": "p-1"})],
        ]);
        let answer = calculate_liquidity(&graph, &scope(), &json!({})).expect("liquidity");
        assert_eq!(answer["reserve_cash"], json!("250.000000"));
        assert_eq!(answer["total_cash"], json!("350.000000"));
    }

    #[test]
    fn a_transfer_of_an_amount_no_float_could_hold_keeps_every_digit() {
        let exact = "123456789012345678901.55";
        let graph = FakeGraph::queue(vec![
            vec![
                account("a-1", "operating", "999999999999999999999.99", "0.000000"),
                account("a-2", "reserve", "0.000000", "0.000000"),
            ],
            vec![json!({"id": "a-1"})],
            vec![json!({"id": "a-2"})],
            vec![json!({"id": "t-1"})],
        ]);
        let answer = create_transfer(
            &graph,
            &scope(),
            &json!({"from_account_id": "a-1", "to_account_id": "a-2", "amount": exact}),
        )
        .expect("transfer");
        assert_eq!(answer["amount"], json!("123456789012345678901.550000"));
        let (_, into) = graph.statement(2);
        assert_eq!(
            into["patch"]["balance"],
            json!("123456789012345678901.550000")
        );
    }

    #[test]
    fn listing_accounts_can_be_narrowed_to_one_kind() {
        let graph = FakeGraph::returning(vec![
            account("a-1", "operating", "10.000000", "0.000000"),
            account("a-2", "reserve", "20.000000", "0.000000"),
        ]);
        let answer =
            list_accounts(&graph, &scope(), &json!({"type": "reserve"})).expect("accounts");
        assert_eq!(answer.as_array().expect("rows").len(), 1);
        assert_eq!(answer[0]["id"], json!("a-2"));
    }
}
