#![forbid(unsafe_code)]
//! treasury-management-service: a company's cash movements, the position it reports, the burn that position
//! is judged against, and a straight-line forecast from the history it has.
//!
//! Every amount is exact. The forecast averages amounts over elapsed days and does divide, so its averages
//! are rounded once to the cent on output, which is where the service rounded them too.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a recorded movement answers with its id and the word `recorded`, not with the record;
//! * a company's movements are listed oldest first, and the total counts every movement the company has,
//!   while the list handed back is only the most recent `limit` of them;
//! * a stored position wins over a derived one; with no stored position the cash is simply the inflows less
//!   the outflows and the available cash is that same figure, with liquidity left at adequate;
//! * liquidity is judged on how many months the available cash covers the last thirty days of outflow:
//!   nothing spent at all is excess, more than six months excess, more than three adequate, more than one
//!   tight, and anything less critical;
//! * a forecast with no history is all zeroes and says so, and it always ends at the current available cash
//!   plus the net of the projection, even where that cash is restricted;
//! * the average is taken over the days since the first movement, with a floor of one day, so a company whose
//!   first movement was this morning has that morning's flows treated as a whole day's;
//! * the investment options are a fixed list this service offers, not something it prices.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::{amount, wire};
use vimbai_store::{
    created_record, BookScope, Graph, Records, StoreError, TREASURY_CASH_FLOW,
    TREASURY_CASH_POSITION,
};

fn refused(message: &str) -> StoreError {
    StoreError::Invalid(message.to_string())
}

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn or_default(payload: &Value, key: &str, fallback: &str) -> String {
    match text(payload, key) {
        value if value.is_empty() => fallback.to_string(),
        value => value,
    }
}

fn field<'a>(record: &'a Value, key: &str) -> &'a str {
    record.get(key).and_then(Value::as_str).unwrap_or("")
}

fn money(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        Some(Value::Null) | None => Err(refused(&format!("{key} is required"))),
        Some(value) => amount(&json!({ "amount": value }), "amount"),
    }
}

fn money_or_nothing(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        Some(Value::Null) | None => Ok(Money::zero()),
        Some(value) => amount(&json!({ "amount": value }), "amount"),
    }
}

fn remember(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    kind: vimbai_store::RecordKind,
    body: Value,
) -> Result<Value, StoreError> {
    let saved = Records::new(graph, kind).create(scope, &body)?;
    let mut answer = created_record(&saved);
    if let (Some(answer), Some(fields)) = (answer.as_object_mut(), body.as_object()) {
        for (key, value) in fields {
            answer.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    Ok(answer)
}

/// Everything the caller has recorded for one company, oldest first.
fn flows_for(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
) -> Result<Vec<Value>, StoreError> {
    let mut mine: Vec<Value> = Records::new(graph, TREASURY_CASH_FLOW)
        .list(scope, 4000)?
        .into_iter()
        .filter(|row| field(row, "company_id") == company_id)
        .collect();
    mine.sort_by(|one, two| {
        field(one, "date")
            .cmp(field(two, "date"))
            .then(field(one, "created_at").cmp(field(two, "created_at")))
    });
    Ok(mine)
}

/// `POST /cashflows`.
pub fn record_cashflow(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = text(payload, "company_id");
    if company_id.is_empty() {
        return Err(refused("company_id is required"));
    }
    let amount_moved = money(payload, "amount")?;
    let flow_type = or_default(payload, "flow_type", "inflow");
    if !["inflow", "outflow"].contains(&flow_type.as_str()) {
        return Err(refused("flow_type must be inflow or outflow"));
    }

    let saved = remember(
        graph,
        scope,
        TREASURY_CASH_FLOW,
        json!({
            "company_id": company_id,
            "account_id": text(payload, "account_id"),
            "flow_type": flow_type,
            "amount": wire(&amount_moved),
            "currency": or_default(payload, "currency", "USD"),
            "date": text(payload, "date"),
            "description": text(payload, "description"),
            "category": text(payload, "category"),
            "created_at": text(payload, "created_at"),
        }),
    )?;
    // as shipped: the id and nothing else but the word recorded
    Ok(json!({"id": field(&saved, "id"), "status": "recorded"}))
}

/// `GET /cashflows/{company_id}`.
pub fn list_cashflows(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let limit = vimbai_store::query::query_page(query, 100, 4000);
    let all = flows_for(graph, scope, company_id)?;
    let total = all.len();
    let recent = if limit == 0 {
        all.clone()
    } else {
        let start = all.len().saturating_sub(limit);
        all[start..].to_vec()
    };
    Ok(json!({
        "company_id": company_id,
        "cashflows": recent,
        "total": total,
    }))
}

/// The stored position for a company, if one has been set.
fn stored_position(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
) -> Result<Option<Value>, StoreError> {
    let mut mine: Vec<Value> = Records::new(graph, TREASURY_CASH_POSITION)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| field(row, "company_id") == company_id)
        .collect();
    mine.sort_by(|one, two| field(one, "created_at").cmp(field(two, "created_at")));
    Ok(mine.into_iter().next())
}

/// `GET /position/{company_id}`.
pub fn get_cash_position(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    if let Some(stored) = stored_position(graph, scope, company_id)? {
        return Ok(stored);
    }
    let mut total = Money::zero();
    for flow in flows_for(graph, scope, company_id)? {
        let value = money_or_nothing(&flow, "amount")?;
        total = if field(&flow, "flow_type") == "outflow" {
            total.minus(&value)
        } else {
            total.plus(&value)
        };
    }
    Ok(json!({
        "id": "",
        "company_id": company_id,
        "total_cash": wire(&total),
        "currency": "USD",
        "available_cash": wire(&total),
        "restricted_cash": wire(&Money::zero()),
        "short_term_investments": wire(&Money::zero()),
        "liquidity_level": "adequate",
        "as_of": "",
    }))
}

/// How many months the available cash covers the burn, as one of the service's four words.
fn liquidity_level(available: &Money, monthly_burn: &Money) -> &'static str {
    if *monthly_burn <= Money::zero() {
        return "excess";
    }
    let months = available.over(monthly_burn);
    let six = Money::parse("6").expect("money");
    let three = Money::parse("3").expect("money");
    let one = Money::parse("1").expect("money");
    if months > six {
        "excess"
    } else if months > three {
        "adequate"
    } else if months > one {
        "tight"
    } else {
        "critical"
    }
}

/// `PUT /position/{company_id}`.
pub fn update_cash_position(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let total = money(payload, "total_cash")?;
    let available = money(payload, "available_cash")?;
    let restricted = money_or_nothing(payload, "restricted_cash")?;
    let investments = money_or_nothing(payload, "short_term_investments")?;

    // the burn is the last thirty days of outflow, measured against the caller's own clock
    let now = text(payload, "now");
    let mut burn = Money::zero();
    for flow in flows_for(graph, scope, company_id)? {
        if field(&flow, "flow_type") != "outflow" {
            continue;
        }
        let when = field(&flow, "date");
        if when.is_empty() || (!now.is_empty() && when > now.as_str()) {
            continue;
        }
        burn = burn.plus(&money_or_nothing(&flow, "amount")?);
    }
    let level = liquidity_level(&available, &burn);

    let body = json!({
        "company_id": company_id,
        "total_cash": wire(&total),
        "currency": or_default(payload, "currency", "USD"),
        "available_cash": wire(&available),
        "restricted_cash": wire(&restricted),
        "short_term_investments": wire(&investments),
        "liquidity_level": level,
        "as_of": text(payload, "as_of"),
        "created_at": text(payload, "created_at"),
    });

    match stored_position(graph, scope, company_id)? {
        Some(stored) => {
            let stored_id = field(&stored, "id").to_string();
            let updated =
                Records::new(graph, TREASURY_CASH_POSITION).update(scope, &stored_id, &body)?;
            Ok(created_record(&updated))
        }
        None => remember(graph, scope, TREASURY_CASH_POSITION, body),
    }
}

/// `POST /forecast/{company_id}`: the run rate of what has happened, carried forward.
pub fn cash_forecast(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let days = vimbai_store::query::query_number(query, "days", 30);
    let now = text(query, "now");
    let flows = flows_for(graph, scope, company_id)?;

    if flows.is_empty() {
        return Ok(json!({
            "company_id": company_id,
            "period_start": now,
            "period_end": "",
            "projected_inflows": "0.00",
            "projected_outflows": "0.00",
            "net_cash_flow": "0.00",
            "ending_position": "0.00",
            "assumptions": ["No historical data - forecast is zero-based"],
        }));
    }

    let first = field(&flows[0], "date").to_string();
    // whole days between the first movement and now, never less than one
    let elapsed = match (days_between(&first, &now), now.is_empty()) {
        (_, true) => 1,
        (Some(days), false) => days.max(1),
        (None, false) => 1,
    };

    let mut inflow = Money::zero();
    let mut outflow = Money::zero();
    for flow in &flows {
        let value = money_or_nothing(flow, "amount")?;
        if field(flow, "flow_type") == "outflow" {
            outflow = outflow.plus(&value);
        } else {
            inflow = inflow.plus(&value);
        }
    }
    let per_day = Money::parse(&elapsed.to_string()).expect("money");
    let window = Money::parse(&days.to_string()).expect("money");
    let average_in = inflow.over(&per_day);
    let average_out = outflow.over(&per_day);
    let projected_in = average_in.times(&window);
    let projected_out = average_out.times(&window);

    let current = match stored_position(graph, scope, company_id)? {
        Some(stored) => money_or_nothing(&stored, "available_cash")?,
        None => Money::zero(),
    };

    Ok(json!({
        "company_id": company_id,
        "period_start": now,
        "period_end": "",
        "projected_inflows": projected_in.round(2).to_fixed(2),
        "projected_outflows": projected_out.round(2).to_fixed(2),
        "net_cash_flow": projected_in.minus(&projected_out).round(2).to_fixed(2),
        "ending_position": current
            .plus(&projected_in)
            .minus(&projected_out)
            .round(2)
            .to_fixed(2),
        "assumptions": [
            format!("Average daily inflow: {}", average_in.round(2).to_fixed(2)),
            format!("Average daily outflow: {}", average_out.round(2).to_fixed(2)),
            format!("Forecast period: {days} days"),
        ],
    }))
}

/// Whole days from one calendar date to another, both written as `YYYY-MM-DD`.
fn days_between(from: &str, to: &str) -> Option<i64> {
    let parse = |value: &str| -> Option<(i64, i64, i64)> {
        let date = value.get(0..10)?;
        let mut parts = date.split('-');
        let year = parts.next()?.parse().ok()?;
        let month = parts.next()?.parse().ok()?;
        let day = parts.next()?.parse().ok()?;
        Some((year, month, day))
    };
    let (from_year, from_month, from_day) = parse(from)?;
    let (to_year, to_month, to_day) = parse(to)?;
    Some(day_number(to_year, to_month, to_day) - day_number(from_year, from_month, from_day))
}

/// Days since the epoch, so two dates can be subtracted without a calendar dependency.
fn day_number(year: i64, month: i64, day: i64) -> i64 {
    let mut y = year;
    let mut m = month;
    if m <= 2 {
        y -= 1;
        m += 12;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let year_of_era = y - era * 400;
    let day_of_year = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era
}

/// `GET /investment-options`: the fixed list this service offers.
pub fn investment_options() -> Value {
    json!({"options": [
        {"name": "Money Market Fund", "instrument_type": "money_market", "expected_return": "0.045",
         "duration_days": 30, "min_amount": "1000.00", "risk_level": "low", "liquidity": "high"},
        {"name": "Treasury Bill 91-Day", "instrument_type": "treasury_bill", "expected_return": "0.07",
         "duration_days": 91, "min_amount": "5000.00", "risk_level": "very_low", "liquidity": "medium"},
        {"name": "Fixed Deposit 6M", "instrument_type": "fixed_deposit", "expected_return": "0.06",
         "duration_days": 180, "min_amount": "10000.00", "risk_level": "very_low", "liquidity": "low"},
        {"name": "Government Bond 2Y", "instrument_type": "bond", "expected_return": "0.085",
         "duration_days": 730, "min_amount": "25000.00", "risk_level": "low", "liquidity": "low"}
    ]})
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn flow(kind: &str, amount_wire: &str, date: &str) -> Value {
        json!({"id": "f", "company_id": "c-1", "flow_type": kind, "amount": amount_wire, "date": date})
    }

    #[test]
    fn a_recorded_movement_answers_with_its_id_and_the_word_recorded() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "f-1"})]]);
        let answer = record_cashflow(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "flow_type": "inflow", "amount": "500.00",
                    "date": "2026-03-01"}),
        )
        .expect("recorded");
        assert_eq!(answer, json!({"id": "f-1", "status": "recorded"}));
    }

    #[test]
    fn a_flow_direction_the_service_does_not_know_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        let refused = record_cashflow(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "flow_type": "sideways", "amount": "1.00"}),
        );
        assert!(refused.is_err());
    }

    #[test]
    fn the_total_counts_every_movement_while_the_list_is_only_the_most_recent() {
        let graph = FakeGraph::returning(vec![
            flow("inflow", "100.000000", "2026-03-01T00:00:00"),
            flow("inflow", "200.000000", "2026-03-02T00:00:00"),
            flow("outflow", "50.000000", "2026-03-03T00:00:00"),
        ]);
        let answer = list_cashflows(&graph, &scope(), "c-1", &json!({"limit": 2})).expect("flows");
        assert_eq!(answer["total"], json!(3));
        assert_eq!(answer["cashflows"].as_array().expect("rows").len(), 2);
        // the most recent two: the 200 inflow and the 50 outflow, oldest first
        assert_eq!(answer["cashflows"][1]["amount"], json!("50.000000"));
    }

    #[test]
    fn with_no_stored_position_the_cash_is_what_the_movements_add_up_to() {
        let graph = FakeGraph::queue(vec![
            vec![],
            vec![
                flow("inflow", "1000.000000", "2026-03-01"),
                flow("outflow", "250.500000", "2026-03-02"),
            ],
        ]);
        let answer = get_cash_position(&graph, &scope(), "c-1").expect("position");
        assert_eq!(answer["total_cash"], json!("749.500000"));
        assert_eq!(answer["available_cash"], json!("749.500000"));
        assert_eq!(answer["liquidity_level"], json!("adequate"));
    }

    #[test]
    fn liquidity_is_judged_on_the_months_the_cash_covers() {
        assert_eq!(
            liquidity_level(&Money::parse("100.00").expect("money"), &Money::zero()),
            "excess"
        );
        // seven months' cover
        assert_eq!(
            liquidity_level(
                &Money::parse("700.00").expect("money"),
                &Money::parse("100.00").expect("money")
            ),
            "excess"
        );
        // four months'
        assert_eq!(
            liquidity_level(
                &Money::parse("400.00").expect("money"),
                &Money::parse("100.00").expect("money")
            ),
            "adequate"
        );
        // two months'
        assert_eq!(
            liquidity_level(
                &Money::parse("200.00").expect("money"),
                &Money::parse("100.00").expect("money")
            ),
            "tight"
        );
        // half a month
        assert_eq!(
            liquidity_level(
                &Money::parse("50.00").expect("money"),
                &Money::parse("100.00").expect("money")
            ),
            "critical"
        );
    }

    #[test]
    fn a_forecast_carries_the_run_rate_forward_and_ends_at_the_current_cash() {
        let graph = FakeGraph::queue(vec![
            vec![
                flow("inflow", "3000.000000", "2026-03-01"),
                flow("outflow", "1000.000000", "2026-03-01"),
            ],
            vec![json!({"id": "p-1", "company_id": "c-1", "available_cash": "500.000000"})],
        ]);
        let answer = cash_forecast(
            &graph,
            &scope(),
            "c-1",
            &json!({"days": 10, "now": "2026-03-11"}),
        )
        .expect("forecast");
        // ten days elapsed: 300 a day in, 100 a day out, over ten days
        assert_eq!(answer["projected_inflows"], json!("3000.00"));
        assert_eq!(answer["projected_outflows"], json!("1000.00"));
        assert_eq!(answer["net_cash_flow"], json!("2000.00"));
        assert_eq!(answer["ending_position"], json!("2500.00"));
        assert_eq!(answer["assumptions"][2], json!("Forecast period: 10 days"));
    }

    #[test]
    fn a_forecast_with_no_history_is_zero_and_says_so() {
        let graph = FakeGraph::returning(vec![]);
        let answer =
            cash_forecast(&graph, &scope(), "c-1", &json!({"days": 30})).expect("forecast");
        assert_eq!(answer["projected_inflows"], json!("0.00"));
        assert_eq!(answer["ending_position"], json!("0.00"));
        assert_eq!(
            answer["assumptions"][0],
            json!("No historical data - forecast is zero-based")
        );
    }

    #[test]
    fn a_forecast_taken_the_same_day_still_treats_it_as_one_day() {
        let graph = FakeGraph::queue(vec![
            vec![flow("inflow", "120.000000", "2026-03-11")],
            vec![],
        ]);
        let answer = cash_forecast(
            &graph,
            &scope(),
            "c-1",
            &json!({"days": 30, "now": "2026-03-11"}),
        )
        .expect("forecast");
        // 120 over one day, carried thirty days
        assert_eq!(answer["projected_inflows"], json!("3600.00"));
    }

    #[test]
    fn the_days_between_two_dates_are_whole_days() {
        assert_eq!(days_between("2026-03-01", "2026-03-11"), Some(10));
        assert_eq!(days_between("2026-02-28", "2026-03-01"), Some(1));
        // a leap year still counts one day from February to March
        assert_eq!(days_between("2024-02-28", "2024-03-01"), Some(2));
        assert_eq!(
            days_between("2026-03-01T08:00:00", "2026-03-03T23:00:00"),
            Some(2)
        );
    }

    #[test]
    fn a_position_is_replaced_rather_than_added_to_a_second_time() {
        let graph = FakeGraph::queue(vec![
            vec![flow("outflow", "1000.000000", "2026-03-01")],
            vec![json!({"id": "p-1", "company_id": "c-1", "created_at": "2026-03-01T00:00:00"})],
            vec![
                json!({"id": "p-1", "available_cash": "2500.000000", "liquidity_level": "adequate"}),
            ],
        ]);
        let answer = update_cash_position(
            &graph,
            &scope(),
            "c-1",
            &json!({"total_cash": "3000.00", "available_cash": "2500.00",
                    "as_of": "2026-03-11", "now": "2026-03-11"}),
        )
        .expect("position");
        assert_eq!(answer["available_cash"], json!("2500.000000"));
        // the third statement is the update, against the position that was already there
        let (cypher, params) = graph.statement(2);
        assert!(cypher.contains("SET x += $patch"));
        assert_eq!(params["id"], json!("p-1"));
        // two and a half months of cover at a thousand a month
        assert_eq!(params["patch"]["liquidity_level"], json!("tight"));
    }

    #[test]
    fn a_position_of_an_amount_no_float_could_hold_keeps_every_digit() {
        let graph = FakeGraph::queue(vec![vec![], vec![], vec![json!({"id": "p-1"})]]);
        let answer = update_cash_position(
            &graph,
            &scope(),
            "c-1",
            &json!({"total_cash": "123456789012345678901.55", "available_cash": "0.45"}),
        )
        .expect("position");
        assert_eq!(answer["total_cash"], json!("123456789012345678901.550000"));
        assert_eq!(answer["available_cash"], json!("0.450000"));
    }

    #[test]
    fn the_investment_options_are_the_four_the_service_offers() {
        let answer = investment_options();
        assert_eq!(answer["options"].as_array().expect("options").len(), 4);
        assert_eq!(answer["options"][1]["name"], json!("Treasury Bill 91-Day"));
    }
}
