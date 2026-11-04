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

#[tokio::test]
async fn croic_keeps_free_cash_flow_exact_for_a_huge_company() {
    let (status, body) = call(
        "/calc/croic",
        Some(json!({
            "company_id": "c1",
            // a quadrillion, with cents the double type could not hold
            "operating_cash_flow": "1234567890123456.78",
            "capital_expenditures": "234567890123.45",
            "total_assets": "9999999999999999.99",
            "current_liabilities": "1000000000000.00",
            "tax_rate": "0.25",
            "periods": ["2024", "2025"],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // the subtraction keeps every cent, and the ratio is only rounded on the way out
    assert_eq!(body["current_croic"]["free_cash_flow"], 1234333322233333.3);
    assert_eq!(
        body["current_croic"]["invested_capital"],
        9998999999999999.99
    );
    // about 0.1234 of invested capital: past 0.10, short of 0.15
    assert_eq!(body["current_croic"]["capital_efficiency_rating"], "good");
}

#[tokio::test]
async fn eva_charges_exact_capital_and_splits_the_segments() {
    let (status, body) = call(
        "/calc/eva",
        Some(json!({
            "company_id": "c1", "period": "2026",
            "ebit": 900_000.0, "tax_rate": 0.25,
            "total_assets": 5_000_000.0, "current_liabilities": 1_000_000.0,
            "debt": 2_000_000.0, "equity": 3_000_000.0,
            "cost_of_debt": 0.06, "cost_of_equity": 0.12,
            "expected_growth": 0.05,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let metrics = &body["eva_metrics"];
    assert_eq!(metrics["invested_capital"], 4_000_000.0);
    assert_eq!(metrics["weighted_average_cost_of_capital"], 0.09);
    assert_eq!(metrics["net_operating_profit_after_tax"], 675_000.0);
    assert_eq!(metrics["capital_charge"], 360_000.0);
    assert_eq!(metrics["economic_value_added"], 315_000.0);
    assert_eq!(body["wealth_created"], true);
    assert_eq!(body["eva_momentum"], "improving");
}

#[tokio::test]
async fn amortization_charges_exact_decimals_down_to_the_cent() {
    let (status, body) = call(
        "/calc/amortization",
        Some(json!({
            "company_id": "c1",
            "period_start": "2026-01-01",
            "period_end": "2026-12-31",
            "intangibles": [
                {"intangible_id": "i1", "type": "software", "cost": 48000.0,
                 "residual_value": 0.0, "useful_life_years": 4,
                 "accumulated_amortization": 10000.0, "rate": 0.25, "method": "straight_line"},
                // a cost whose reducing-balance charge does not divide evenly
                {"intangible_id": "i2", "type": "patent", "cost": "123456.79",
                 "residual_value": "5678.91", "useful_life_years": 7,
                 "accumulated_amortization": "20000.05", "rate": "0.2",
                 "method": "reducing_balance"},
            ],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["period"]["start"], "2026-01-01");
    let patent = &body["schedules"][1];
    // (123456.79 - 20000.05) * 0.2 = 20691.348, to the cent
    assert_eq!(patent["annual_amortization"], 20691.35);
    assert_eq!(patent["monthly_amortization"], 1724.28);
    assert_eq!(patent["net_book_value"], 82765.39);
    // totals are summed as decimals, not folded in a double
    assert_eq!(body["total_amortization"], 32691.35);
    assert_eq!(body["total_net_book_value"], 108765.39);
}

#[tokio::test]
async fn depreciation_totals_the_register_as_decimals() {
    let (status, body) = call(
        "/calc/depreciation",
        Some(json!({
            "company_id": "c1",
            "method": "straight_line",
            "period_start": "2026-01-01", "period_end": "2026-12-31",
            "assets": [
                {"asset_id": "a1", "cost": 10000.0, "salvage": 1000.0, "useful_life_years": 5},
                {"asset_id": "a2", "cost": "1234567.89", "salvage": "1234.56",
                 "useful_life_years": 7},
            ],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // (1234567.89 - 1234.56) / 7 = 176190.4757..., to the cent
    assert_eq!(
        body["asset_depreciation"][1]["annual_depreciation"],
        176190.48
    );
    // 1800 + 176190.4757 = 177990.4757: the register total is exact before rounding
    assert_eq!(body["total_depreciation"], 177990.48);
    // the forecast line is total * 0.9, and 0.9 here is exactly nine tenths
    assert_eq!(body["schedule"][0]["depreciation"], 160191.43);
    assert_eq!(body["schedule"].as_array().unwrap().len(), 5);
}

#[tokio::test]
async fn depreciation_method_walks_the_carrying_amount_down() {
    let (status, body) = call(
        "/calc/depreciation-method",
        Some(json!({
            "cost": "1234567.89", "salvage_value": "1234.56",
            "useful_life": 7, "method": "double_declining",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["annual_depreciation"], 352733.68);
    // the schedule keeps the name the endpoint has always published
    assert_eq!(body["depreciation_schedule"][1]["accumulated"], 705467.37);
    assert_eq!(body["depreciation_schedule"][1]["book_value"], 529100.52);
    assert_eq!(body["depreciation_schedule"].as_array().unwrap().len(), 7);
}

#[tokio::test]
async fn fixed_assets_reports_a_register_in_decimals() {
    let (status, body) = call(
        "/calc/fixed-assets",
        Some(json!({
            "company_id": "c1",
            "capital_budget": "200000.00",
            "target_capitalization_rate": "0.4",
            "assets": [
                {"asset_id": "a1", "asset_name": "Truck", "category": "vehicle",
                 "purchase_date": "2022-01-01", "purchase_cost": 60000.0, "salvage_value": 6000.0,
                 "useful_life_years": 5, "depreciation_method": "straight_line",
                 "accumulated_depreciation": 36000.0},
                {"asset_id": "a2", "asset_name": "Rig", "category": "equipment",
                 "purchase_date": "2021-06-30", "purchase_cost": "1234567.89",
                 "salvage_value": "12345.67", "useful_life_years": 10,
                 "depreciation_method": "straight_line",
                 "accumulated_depreciation": "458123.45"},
            ],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // 60000.00 + 1234567.89 summed as a decimal, not folded in a double
    assert_eq!(body["asset_summary"]["total_cost"], 1294567.89);
    assert_eq!(body["asset_summary"]["net_book_value"], 800444.44);
    // (1234567.89 - 12345.67) / 10 = 122222.222, to the cent
    assert_eq!(
        body["depreciation_schedule"][1]["annual_depreciation"],
        122222.22
    );
    assert_eq!(body["asset_category_analysis"][1]["net_value"], 776444.44);
    // replacement needs are a tenth of cost, expansion a third of the budget
    assert_eq!(
        body["capital_expenditure_plan"]["replacement_needs"],
        129456.79
    );
    assert_eq!(
        body["capital_expenditure_plan"]["expansion_allocations"],
        60000.0
    );
}

#[tokio::test]
async fn goodwill_impairment_caps_the_loss_at_goodwill_first() {
    let (status, body) = call(
        "/calc/goodwill",
        Some(json!({
            "company_id": "c1", "fiscal_year": 2026, "test_method": "higher_of",
            "cgu": {"name": "cgu1", "carrying_value": "1000.00", "fair_value": "700.00",
                    "value_in_use": "750.00", "goodwill_allocated": "200.00"},
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["recoverable_amount"], 750.0);
    assert_eq!(body["impairment_loss"], 250.0);
    assert_eq!(body["goodwill_impaired"], 200.0);
    assert_eq!(body["other_assets_impaired"], 50.0);
    assert_eq!(body["post_impairment_carrying"], 750.0);
    assert_eq!(body["is_impaired"], true);

    // a one-cent shortfall is a real impairment, and it hits goodwill only
    let (_, cent) = call(
        "/calc/goodwill",
        Some(json!({
            "company_id": "c1", "fiscal_year": 2026, "test_method": "fair_value_only",
            "cgu": {"name": "cgu2", "carrying_value": "1234567.89", "fair_value": "1234567.88",
                    "value_in_use": "900000.00", "goodwill_allocated": "250000.00"},
        })),
    )
    .await;
    assert_eq!(cent["impairment_loss"], 0.01);
    assert_eq!(cent["other_assets_impaired"], 0.0);
    assert_eq!(cent["post_impairment_carrying"], 1234567.88);
}

#[tokio::test]
async fn inventory_valuation_costs_the_same_register_the_same_way() {
    let register = json!({
        "company_id": "c1", "method": "fifo",
        "opening_inventory": 5000.0, "opening_qty": 50,
        "purchases": [
            {"date": "2026-02-01", "quantity": 100, "unit_cost": 110.0},
            {"date": "2026-01-15", "quantity": 100, "unit_cost": 100.0},
        ],
        "sales": [{"date": "2026-03-01", "quantity": 120, "unit_price": 200.0}],
    });
    let (status, body) = call("/calc/inventory-valuation", Some(register)).await;
    assert_eq!(status, StatusCode::OK);
    // opening stock goes first: 50 @ 100 plus 70 of the January layer
    assert_eq!(body["cost_of_goods_sold"], 12000.0);
    assert_eq!(body["ending_inventory"], 14000.0);
    assert_eq!(body["gross_margin"], 50.0);
}

#[tokio::test]
async fn inventory_lifo_book_value_is_exact_even_on_a_cent_tie() {
    // total cost 10142.494, LIFO cogs 9041.949, so book value is 1100.545.
    // The Python service folds this in a double, lands a hair above the tie and
    // rounds up to 1100.55. Exactly, it is a tie, and a tie rounds to even.
    let (status, body) = call(
        "/calc/inventory-valuation",
        Some(json!({
            "company_id": "c1", "method": "lifo",
            "opening_inventory": 1000.55, "opening_qty": 3,
            "purchases": [
                {"date": "2026-02-01", "quantity": 7, "unit_cost": 1234.567},
                {"date": "2026-01-15", "quantity": 5, "unit_cost": 99.995},
            ],
            "sales": [{"date": "2026-03-01", "quantity": 11, "unit_price": 2000.0}],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_goods_available"], 10142.49);
    assert_eq!(body["cost_of_goods_sold"], 9041.95);
    assert_eq!(body["ending_inventory"], 1100.54);
}

#[tokio::test]
async fn net_realizable_value_subtracts_at_cost() {
    let (status, body) = call(
        "/calc/net-realizable-value",
        Some(json!({
            "selling_Price": 500.0, "Completion_Cost": 120.0, "selling_cost": 30.0,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["net_realizable_value"], 350.0);
    assert_eq!(body["formula"], "500.0 - 120.0 - 30.0 = 350.0");

    // two subtractions that land on a round figure, exactly
    let (_, drift) = call(
        "/calc/net-realizable-value",
        Some(json!({"selling_price": 100.10, "completion_cost": 15.05, "selling_cost": 5.02})),
    )
    .await;
    assert_eq!(drift["net_realizable_value"], 80.03);

    // where a double runs out of room: 0.3 - 0.1 - 0.1 is 0.09999999999999998
    // in Python, and a tenth of a unit here
    let (_, tenth) = call(
        "/calc/net-realizable-value",
        Some(json!({"selling_price": 0.3, "completion_cost": 0.1, "selling_cost": 0.1})),
    )
    .await;
    assert_eq!(tenth["net_realizable_value"], 0.1);
}

#[tokio::test]
async fn nrv_scrap_residual_and_write_down_paths_answer() {
    let (_, scrap) = call(
        "/calc/nrv-scrap-value",
        Some(json!({
            "original_cost": 100000, "accumulated_depreciation": 60000,
            "disposal_cost": 2000, "proceeds_from_sale": 30000,
        })),
    )
    .await;
    assert_eq!(scrap["net_book_value"], 40000.0);
    assert_eq!(scrap["scrap_value"], 28000.0);
    assert_eq!(scrap["gain_loss_on_disposal"], -12000.0);

    let (_, residual) = call(
        "/calc/nrv-residual-value",
        Some(json!({"original_cost": 100000, "depreciation_rate": 0.25, "useful_life_years": 5})),
    )
    .await;
    assert_eq!(residual["total_depreciation"], 125000.0);
    assert_eq!(residual["residual_value"], 0.0);

    let (_, inventory) = call(
        "/calc/nrv-inventory",
        Some(json!({
            "estimated_selling_price": 250, "estimated_completion_cost": 40,
            "estimated_selling_expenses": 10,
        })),
    )
    .await;
    assert_eq!(inventory["net_realizable_value"], 200.0);

    let (_, compared) = call(
        "/calc/nrv-compare-cost",
        Some(json!({"cost": 100, "selling_price": 100, "completion_cost": 10, "selling_cost": 5})),
    )
    .await;
    assert_eq!(compared["net_realizable_value"], 85.0);
    assert_eq!(compared["lower_of_cost_or_nrv"], 85.0);
    assert_eq!(compared["write_down_required"], true);
    assert_eq!(compared["write_down_amount"], 15.0);

    // a non-positive NRV leaves the cost in place rather than valuing stock at nil
    let (_, nil) = call(
        "/calc/nrv-compare-cost",
        Some(json!({"cost": 100, "selling_price": 50, "completion_cost": 60, "selling_cost": 5})),
    )
    .await;
    assert_eq!(nil["lower_of_cost_or_nrv"], 100.0);
    assert_eq!(nil["write_down_amount"], 115.0);
}
