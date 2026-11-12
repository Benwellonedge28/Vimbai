#![forbid(unsafe_code)]
//! treasury-reporting-service: the figures a treasury reports on, gathered into a report and kept.
//!
//! Three shapes of report - a cash position, an fx exposure and a debt portfolio - plus the listing that
//! hands them back. Each generator reads the entries the caller supplies, adds them up exactly, and stores the
//! report with its summary, so a figure published here is the figure that was computed, not a formatting of it.
//!
//! Shapes kept from the service: the cash position totals the usd balances and groups the native balances by
//! currency; the fx report totals the exposure and the unhedged amount and averages the hedge ratio over the
//! pairs it was given, with no pairs reporting no average rather than dividing; the debt report publishes the
//! figures it was handed and counts the instruments; and the listing filters by report type and hands back the
//! most recent `limit` reports in the order they were generated.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::query::query_page;
use vimbai_store::{created_record, BookScope, Graph, Records, StoreError, TREASURY_REPORT};

use crate::helpers::{field, money_or_nothing, or_default, text};

/// The report types the service generated; anything else is a listing filter, not a generator.
pub const REPORT_TYPES: [&str; 5] = [
    "cash_position",
    "fx_exposure",
    "debt_portfolio",
    "liquidity",
    "investment_summary",
];

/// Remember a report and answer with it.
fn remember<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    body: Value,
) -> Result<Value, StoreError> {
    let saved = Records::new(graph, TREASURY_REPORT).create(scope, &body)?;
    Ok(created_record(&saved))
}

/// The entries of a report, as rows to add up.
fn rows(payload: &Value, key: &str) -> Vec<Value> {
    payload
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// `POST /reports/cash-position`: what the accounts held, in their own currencies and in usd.
pub fn cash_position<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let entries = rows(payload, "entries");
    let mut total_usd = Money::zero();
    let mut by_currency: Vec<(String, Money)> = Vec::new();

    for entry in &entries {
        total_usd = total_usd.plus(&money_or_nothing(entry, "balance_usd")?);
        let currency = field(entry, "currency").to_string();
        let balance = money_or_nothing(entry, "balance")?;
        match by_currency.iter_mut().find(|(name, _)| *name == currency) {
            Some((_, running)) => *running = running.plus(&balance),
            None => by_currency.push((currency, balance)),
        }
    }

    let totals: Value = by_currency
        .iter()
        .map(|(currency, amount)| (currency.clone(), wire(amount)))
        .collect::<serde_json::Map<String, Value>>()
        .into();

    remember(
        graph,
        scope,
        json!({
            "report_type": "cash_position",
            "period": text(payload, "period"),
            "data": {"entries": entries},
            "summary": {
                "total_cash_usd": wire(&total_usd),
                "total_accounts": entries.len(),
                "by_currency": totals,
            },
            "generated_at": text(payload, "generated_at"),
            "generated_by": or_default(payload, "generated_by", ""),
        }),
    )
}

/// `POST /reports/fx-exposure`: what is exposed to currency movement, and how much of it is hedged.
pub fn fx_exposure<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let entries = rows(payload, "entries");
    let mut total_exposure = Money::zero();
    let mut total_unhedged = Money::zero();
    let mut hedge_total = Money::zero();

    for entry in &entries {
        total_exposure = total_exposure.plus(&money_or_nothing(entry, "exposure_usd")?);
        total_unhedged = total_unhedged.plus(&money_or_nothing(entry, "unhedged_amount")?);
        hedge_total = hedge_total.plus(&money_or_nothing(entry, "hedge_ratio")?);
    }

    // an average over no pairs is no average, not a division by nothing
    let average = if entries.is_empty() {
        Money::zero()
    } else {
        hedge_total
            .over(&Money::from_i64(entries.len() as i64))
            .round(6)
    };

    remember(
        graph,
        scope,
        json!({
            "report_type": "fx_exposure",
            "period": text(payload, "period"),
            "data": {"entries": entries},
            "summary": {
                "total_exposure_usd": wire(&total_exposure),
                "total_unhedged_usd": wire(&total_unhedged),
                "average_hedge_ratio": wire(&average),
                "currency_pairs": entries.len(),
            },
            "generated_at": text(payload, "generated_at"),
            "generated_by": or_default(payload, "generated_by", ""),
        }),
    )
}

/// `POST /reports/debt-portfolio`: the debt as it stands, with the instruments behind it.
pub fn debt_portfolio<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let instruments = rows(payload, "debt_instruments");
    let total_debt = money_or_nothing(payload, "total_debt")?;
    let total_usd = money_or_nothing(payload, "total_debt_usd")?;
    let rate = money_or_nothing(payload, "weighted_avg_rate")?;

    remember(
        graph,
        scope,
        json!({
            "report_type": "debt_portfolio",
            "period": text(payload, "period"),
            "data": {"instruments": instruments},
            "summary": {
                "total_debt": wire(&total_debt),
                "total_debt_usd": wire(&total_usd),
                "weighted_avg_rate": wire(&rate),
                "instrument_count": instruments.len(),
            },
            "generated_at": text(payload, "generated_at"),
            "generated_by": or_default(payload, "generated_by", ""),
        }),
    )
}

/// `GET /reports`: the caller's reports, newest last, narrowed to one type when asked.
pub fn list_reports<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = crate::helpers::optional(query, "report_type");
    let limit = query_page(query, 50, 2000);
    let mut all: Vec<Value> = Records::new(graph, TREASURY_REPORT)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            wanted
                .as_deref()
                .map(|kind| field(row, "report_type") == kind)
                .unwrap_or(true)
        })
        .collect();
    all.sort_by(|one, two| field(one, "generated_at").cmp(field(two, "generated_at")));
    let start = all.len().saturating_sub(limit);
    Ok(Value::Array(all[start..].to_vec()))
}

/// `GET /reports/{id}`: one report, for a caller who can see it.
pub fn get_report<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    report_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, TREASURY_REPORT)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == report_id)
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
    fn a_cash_position_report_totals_usd_and_groups_native_balances() {
        let graph = FakeGraph::returning(vec![json!({"id": "r-1"})]);
        let answer = cash_position(
            &graph,
            &scope(),
            &json!({
                "period": "2026-03",
                "entries": [
                    {"account_id": "a", "account_name": "Main", "currency": "USD",
                     "balance": "100.00", "balance_usd": "100.00"},
                    {"account_id": "b", "account_name": "Nostro", "currency": "EUR",
                     "balance": "50.00", "balance_usd": "54.00"},
                    {"account_id": "c", "account_name": "Second EUR", "currency": "EUR",
                     "balance": "25.50", "balance_usd": "27.54"}
                ],
            }),
        )
        .expect("report");

        assert_eq!(answer["id"], json!("r-1"));

        let (statement, params) = graph.last();
        assert!(statement.contains("OWNS_TREASURY_REPORT"), "{statement}");
        let summary = &params["props"]["summary"];
        assert_eq!(params["props"]["report_type"], json!("cash_position"));
        assert_eq!(params["props"]["period"], json!("2026-03"));
        assert_eq!(summary["total_cash_usd"], json!("181.540000"));
        assert_eq!(summary["total_accounts"], json!(3));
        // the native balances add up per currency, not converted
        assert_eq!(summary["by_currency"]["EUR"], json!("75.500000"));
    }

    #[test]
    fn an_average_hedge_ratio_over_no_pairs_is_no_average() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "r-1"})], vec![json!({"id": "r-2"})]]);
        let empty = fx_exposure(
            &graph,
            &scope(),
            &json!({"period": "2026-03", "entries": []}),
        )
        .expect("report");
        assert_eq!(empty["id"], json!("r-1"));
        let (_, params) = graph.last();
        assert_eq!(
            params["props"]["summary"]["average_hedge_ratio"],
            json!("0.000000")
        );
        assert_eq!(params["props"]["summary"]["currency_pairs"], json!(0));

        let some = fx_exposure(
            &graph,
            &scope(),
            &json!({"period": "2026-03", "entries": [
                {"currency_pair": "EUR/USD", "exposure_amount": "1000.00",
                 "exposure_usd": "1080.00", "hedge_ratio": "0.75", "unhedged_amount": "250.00"},
                {"currency_pair": "GBP/USD", "exposure_amount": "500.00",
                 "exposure_usd": "630.00", "hedge_ratio": "0.50", "unhedged_amount": "250.00"}
            ]}),
        )
        .expect("report");
        assert_eq!(some["id"], json!("r-2"));
        let (_, params) = graph.last();
        let summary = &params["props"]["summary"];
        assert_eq!(summary["total_exposure_usd"], json!("1710.000000"));
        assert_eq!(summary["total_unhedged_usd"], json!("500.000000"));
        // 0.75 + 0.50 over two pairs
        assert_eq!(summary["average_hedge_ratio"], json!("0.625000"));
    }

    #[test]
    fn a_debt_report_publishes_what_it_was_handed_and_counts_the_instruments() {
        let graph = FakeGraph::returning(vec![json!({"id": "r-1"})]);
        let answer = debt_portfolio(
            &graph,
            &scope(),
            &json!({
                "period": "2026-Q1",
                "total_debt": "5000000.00",
                "total_debt_usd": "5000000.00",
                "weighted_avg_rate": "0.0875",
                "debt_instruments": [{"id": "i-1"}, {"id": "i-2"}],
            }),
        )
        .expect("report");
        assert_eq!(answer["id"], json!("r-1"));
        let (_, params) = graph.last();
        let summary = &params["props"]["summary"];
        assert_eq!(summary["total_debt"], json!("5000000.000000"));
        assert_eq!(summary["weighted_avg_rate"], json!("0.087500"));
        assert_eq!(summary["instrument_count"], json!(2));
    }

    #[test]
    fn the_listing_narrows_by_type_and_keeps_the_most_recent() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "r-1", "report_type": "cash_position", "generated_at": "2026-03-01T00:00:00"}),
            json!({"id": "r-2", "report_type": "fx_exposure", "generated_at": "2026-03-02T00:00:00"}),
            json!({"id": "r-3", "report_type": "cash_position", "generated_at": "2026-03-03T00:00:00"}),
        ]);
        let all = list_reports(&graph, &scope(), &json!({})).expect("reports");
        assert_eq!(all.as_array().expect("rows").len(), 3);

        let narrowed = list_reports(
            &graph,
            &scope(),
            &json!({"report_type": "cash_position", "limit": "1"}),
        )
        .expect("reports");
        let rows = narrowed.as_array().expect("rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], json!("r-3"));
    }

    #[test]
    fn one_report_is_readable_and_another_book_s_report_is_a_miss() {
        let graph =
            FakeGraph::returning(vec![json!({"id": "r-1", "report_type": "cash_position"})]);
        assert_eq!(
            get_report(&graph, &scope(), "r-1").expect("report")["id"],
            json!("r-1")
        );

        let elsewhere = FakeGraph::returning(vec![]);
        assert!(matches!(
            get_report(&elsewhere, &scope(), "r-9"),
            Err(StoreError::NotFound)
        ));
    }
}
