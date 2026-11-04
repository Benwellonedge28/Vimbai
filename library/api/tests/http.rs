//! The HTTP surface answers the same payload the library does.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn call(uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = match body {
        Some(payload) => Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap(),
        None => Request::builder().uri(uri).body(Body::empty()).unwrap(),
    };
    let response = vimbai_api::router().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn health_names_the_engine_and_counts_the_calculators() {
    let (status, body) = call("/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["engine"], "rust");
    assert!(body["calculators"].as_u64().unwrap() >= 2);
}

#[tokio::test]
async fn the_list_matches_what_answers() {
    let (status, body) = call("/calculators", None).await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<String> = body["calculators"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"target-costing".to_string()));
    assert!(names.contains(&"internal-control-evaluation".to_string()));
}

#[tokio::test]
async fn target_costing_keeps_an_amount_larger_than_a_float_can_hold() {
    let (status, body) = call(
        "/calc/target-costing",
        Some(json!({
            "company_id": "c1",
            "product_name": "Widget",
            "target_selling_price": "123456789012345678901234.56",
            "desired_profit_margin_pct": "12.5",
            "current_cost": "100000000000000000000000.00",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // a tenth of a percent of this number would be invisible in an f64
    assert_eq!(body["target_profit"], "15432098626543209862654.32");
    assert_eq!(body["target_cost"], "108024690385802469038580.24");
}

#[tokio::test]
async fn an_unknown_calculator_says_so_and_lists_the_rest() {
    let (status, body) = call("/calc/nope", Some(json!({}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["calculators"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
async fn controls_still_answer_through_the_wire() {
    let (status, body) = call(
        "/calc/internal-control-evaluation",
        Some(json!({
            "audit_id": "a1",
            "controls_to_test": [
                {"id": "C1", "deficiency": "none"},
                {"id": "C2", "deficiency": "material"},
            ],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["effective_controls"], 1);
    assert_eq!(body["overall_assessment"], "ineffective_with_deficiencies");
}

#[tokio::test]
async fn payment_optimization_decides_on_the_wire() {
    let (status, body) = call(
        "/calc/payment-optimization",
        Some(json!({
            "company_id": "c1",
            "available_cash": "100000",
            "payments": [
                {"payment_id": "p1", "amount": "50000", "due_date": "2026-03-31",
                 "discount_available": "10", "discount_days": 10},
            ],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["payment_schedule"][0]["action"], "PAY_EARLY");
    // the wire keeps the decimal: a client never sees 50000.0
    assert_eq!(body["payment_schedule"][0]["amount"], "50000.00");
}

#[tokio::test]
async fn substantive_testing_projects_over_the_wire() {
    let (status, body) = call(
        "/calc/substantive-testing",
        Some(json!({
            "audit_id": "a1",
            "account_id": "acc1",
            "sample_size": 4,
            "misstatement_threshold": "50",
            "population": [
                {"item_id": "1", "amount": "1000", "misstatement": "100"},
                {"item_id": "2", "amount": "1000", "misstatement": "0"},
                {"item_id": "3", "amount": "1000", "misstatement": "0"},
                {"item_id": "4", "amount": "1000", "misstatement": "0"},
                {"item_id": "5", "amount": "1000", "misstatement": "0"},
            ],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_misstatement"], "100.00");
    assert_eq!(body["projected_misstatement"], "125.00");
    assert_eq!(body["conclusion"], "qualified");
}

#[tokio::test]
async fn divestiture_keeps_every_cent() {
    let (status, body) = call(
        "/calc/divestiture",
        Some(json!({
            "company_id": "c1",
            "subsidiary_name": "Sub",
            "disposal_method": "sale",
            "disposal_proceeds": "123456789012345678901234.56",
            "disposal_costs": "0.01",
            "carrying_value": "100000000000000000000000",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["net_proceeds"], "123456789012345678901234.55");
    assert_eq!(body["gain_or_loss"], "23456789012345678901234.55");
}

#[tokio::test]
async fn capex_budget_funds_against_an_exact_remainder() {
    let (status, body) = call(
        "/calc/capital-expenditure-budget",
        Some(json!({
            "company_id": "c1",
            "budget_year": "2026",
            "total_capex_budget": "123456789012345678901.01",
            "projects": [
                {"project_id": "a", "project_name": "A", "priority": 1,
                 "initial_investment": "123456789012345678900.99",
                 "annual_cash_flow": "1000", "project_life": "2"},
                {"project_id": "b", "project_name": "B", "priority": 2,
                 "initial_investment": "0.02", "annual_cash_flow": "5", "project_life": "2"},
            ],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // the two cents left over fund the second project exactly
    assert_eq!(body["funded_projects"].as_array().unwrap().len(), 2);
    assert_eq!(body["budget_year"], "2026");
}

#[tokio::test]
async fn eps_keeps_whole_shares_and_exact_cents() {
    let (status, body) = call(
        "/calc/eps",
        Some(json!({
            "company_id": "c1",
            "period": "2026",
            "net_profit_attributable": "123456789012345678901234.56",
            "weighted_average_shares": "1000",
            "discontinued_operations_profit": "0",
            "potential_dilutive_shares": [
                {"id": "o1", "number_of_options": "10000",
                 "exercise_price": "40", "average_market_price": "50"}
            ],
            "anti_dilutive_items": [],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // 20% of the options are free shares, exactly, and the count stays an integer
    assert_eq!(body["dilutive_effect"], 2000);
    assert_eq!(body["diluted_shares"], 3000);
    assert_eq!(body["period"], "2026");
    // 123,456,789,012,345,678,901,234.56 / 1,000 keeps every cent
    assert_eq!(body["basic_eps"], "123456789012345678901.2346");
}

#[tokio::test]
async fn du_pont_decomposes_and_calls_the_industry_gap() {
    let (status, body) = call(
        "/calc/du-pont",
        Some(json!({
            "company_id": "c1", "period": "2026",
            "revenue": 1_000_000.0, "net_income": 120_000.0, "ebt": 160_000.0,
            "ebit": 200_000.0, "total_assets": 800_000.0, "total_equity": 400_000.0,
            "total_debt": 400_000.0,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let metrics = &body["du_pont_metrics"];
    assert_eq!(metrics["net_profit_margin"], 0.12);
    assert_eq!(metrics["asset_turnover"], 1.25);
    assert_eq!(metrics["financial_leverage"], 2.0);
    // the three factors multiply back to ROE
    let roe = metrics["return_on_equity"].as_f64().unwrap();
    let product = 0.12 * 1.25 * 2.0;
    assert!((roe - product).abs() < 1e-9);
    assert_eq!(body["conclusions"]["profitability"], "Strong");
    assert_eq!(body["peer_comparison"]["industry_avg_roe"], 15.0);
}

#[tokio::test]
async fn turnover_reports_a_whole_day_count() {
    let (status, body) = call(
        "/calc/asset-turnover",
        Some(json!({
            "company_id": "c1", "period": "2026",
            "revenue": 900_000.0, "cost_of_goods_sold": 600_000.0,
            "total_assets": 500_000.0, "fixed_assets": 200_000.0,
            "current_assets": 300_000.0, "current_liabilities": 150_000.0,
            "accounts_receivable": 120_000.0, "inventory": 90_000.0,
            "accounts_payable": 80_000.0,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["turnover_metrics"]["total_asset_turnover"], 1.8);
    // dso 48.67 + dio 54.75 - dpo 48.67, truncated to whole days
    assert_eq!(body["turnover_metrics"]["cash_conversion_cycle"], 54);
    assert_eq!(body["efficiency_assessment"], "efficient");
}
