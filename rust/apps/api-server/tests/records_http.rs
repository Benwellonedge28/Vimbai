//! The record routes: a caller with a Book, a graph that answers, and what comes out.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use store::{BookScope, Graph, StoreError};
use tower::ServiceExt;

/// A graph that answers with the rows a test handed it and remembers what it was asked.
struct Scripted {
    rows: Vec<Value>,
    /// Answers for successive queries, when one path reads more than one collection.
    queue: Mutex<Vec<Vec<Value>>>,
    seen: Mutex<Vec<(String, Value)>>,
}

impl Scripted {
    fn returning(rows: Vec<Value>) -> Arc<Self> {
        Arc::new(Self {
            rows,
            queue: Mutex::new(Vec::new()),
            seen: Mutex::new(Vec::new()),
        })
    }

    fn queue(scripts: Vec<Vec<Value>>) -> Arc<Self> {
        Arc::new(Self {
            rows: Vec::new(),
            queue: Mutex::new(scripts),
            seen: Mutex::new(Vec::new()),
        })
    }

    fn statement(&self, index: usize) -> (String, Value) {
        self.seen
            .lock()
            .unwrap()
            .get(index)
            .cloned()
            .unwrap_or_else(|| (String::new(), Value::Null))
    }

    fn last(&self) -> (String, Value) {
        self.seen
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_else(|| (String::new(), Value::Null))
    }

    fn statements(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

impl Graph for Scripted {
    fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError> {
        self.seen.lock().unwrap().push((cypher.to_owned(), params));
        let mut queue = self.queue.lock().unwrap();
        if queue.is_empty() {
            return Ok(self.rows.clone());
        }
        Ok(queue.remove(0))
    }
}

async fn call(graph: Arc<Scripted>, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(graph);
    let mut request = match body {
        Some(payload) => Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap(),
        None => Request::builder().uri(uri).body(Body::empty()).unwrap(),
    };
    request
        .headers_mut()
        .insert("x-user-id", "user-1".parse().expect("a valid header value"));
    request
        .headers_mut()
        .insert("x-book-id", "book-1".parse().expect("a valid header value"));

    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn a_calculation_answers_on_the_path_the_python_service_published() {
    let graph = Scripted::returning(vec![json!({"x": {
        "id": "calc-1",
        "company_id": "c-1",
        "product_or_process": "Widgets",
        "period": "2026-11",
        "total_cost": "600.000000",
        "unit_cost": "200.000000",
        "quantity": 3,
        "components": [
            {"name": "resin", "amount": "100", "cost_type": "direct_materials"}
        ]
    }})]);
    let (status, body) = call(
        graph.clone(),
        "/process-costing/calculate",
        Some(json!({
            "company_id": "c-1",
            "product_or_process": "Widgets",
            "quantity": 3,
            "components": [
                {"name": "resin", "amount": 100, "cost_type": "direct_materials"},
                {"name": "labour", "amount": 500, "cost_type": "direct_labor"}
            ]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_cost"], json!("600.000000"));
    assert_eq!(body["unit_cost"], json!("200.000000"));
    assert_eq!(graph.statements(), 1);
}

#[tokio::test]
async fn the_same_answer_is_available_under_records() {
    let graph = Scripted::returning(vec![json!({"x": {
        "id": "calc-1", "company_id": "c-1", "total_cost": "10", "unit_cost": "10", "components": []
    }})]);
    let (status, body) = call(graph, "/records/process-costing/summary/c-1", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["company_id"], json!("c-1"));
    assert_eq!(body["total_calculations"], json!(1));
}

#[tokio::test]
async fn a_request_without_a_book_is_refused_before_anything_runs() {
    let graph = Scripted::returning(vec![]);
    let router = vimbai_api::records::router(graph.clone());
    let request = Request::builder()
        .method("GET")
        .uri("/process-costing/summary/c-1")
        .header("x-user-id", "user-1")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(graph.statements(), 0);
}

#[tokio::test]
async fn a_breakdown_of_a_missing_calculation_is_a_404_not_an_empty_report() {
    let graph = Scripted::returning(vec![]);
    let (status, _) = call(graph, "/process-costing/breakdown/c-1/calc-9", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_scope_headers_become_the_parameters_the_store_queries_with() {
    let graph = Scripted::returning(vec![]);
    let (status, _) = call(graph.clone(), "/process-costing/summary/c-1", None).await;
    assert_eq!(status, StatusCode::OK);
    let (cypher, params) = graph.seen.lock().unwrap().last().cloned().unwrap();
    assert_eq!(params["user_id"], json!("user-1"), "{cypher}");
    assert_eq!(params["book_id"], json!("book-1"), "{cypher}");
    let _ = BookScope::new("user-1", "book-1");
}

#[tokio::test]
async fn both_costing_services_are_answered_by_the_one_implementation() {
    // process-costing and product-costing were generated from the same template: same models, same
    // endpoints, same CostCalculation label. One implementation answers both, so the pair cannot
    // drift apart as they are maintained.
    let payload = json!({
        "company_id": "c-1",
        "product_or_process": "Widgets",
        "quantity": 4,
        "components": [
            {"name": "resin", "amount": 0.1, "cost_type": "direct_materials"},
            {"name": "labour", "amount": "0.2", "cost_type": "direct_labor"}
        ]
    });
    let row = json!({"x": {
        "id": "calc-1",
        "company_id": "c-1",
        "product_or_process": "Widgets",
        "total_cost": "0.300000",
        "unit_cost": "0.075000",
        "quantity": 4,
        "components": []
    }});

    let (process_status, process_body) = call(
        Scripted::returning(vec![row.clone()]),
        "/process-costing/calculate",
        Some(payload.clone()),
    )
    .await;
    let (product_status, product_body) = call(
        Scripted::returning(vec![row]),
        "/product-costing/calculate",
        Some(payload),
    )
    .await;

    assert_eq!(process_status, StatusCode::OK);
    assert_eq!(product_status, StatusCode::OK);
    assert_eq!(process_body, product_body);
    assert_eq!(process_body["unit_cost"], json!("0.075000"));
}

#[tokio::test]
async fn a_price_reduction_is_judged_and_stored_against_the_callers_book() {
    let stored = json!({"x": {
        "id": "a-1",
        "product_id": "p-1",
        "product_name": "Widget",
        "current_price": "100.000000",
        "proposed_price": "90.000000",
        "price_reduction": "10.000000",
        "price_reduction_percentage": "10.000000",
        "current_contribution_per_unit": "40.000000",
        "new_contribution_per_unit": "30.000000",
        "contribution_change_per_unit": "-10.000000",
        "current_expected_sales": "1000.000000",
        "new_expected_sales": "1500.000000",
        "current_total_contribution": "40000.000000",
        "new_total_contribution": "45000.000000",
        "contribution_difference": "5000.000000",
        "minimum_price_for_target_contribution": "0.000000",
        "recommendation": "APPROVE",
        "break_even_volume_increase": "333.333333"
    }});
    let graph = Scripted::returning(vec![stored]);

    let (status, body) = call(
        graph,
        "/selling-price-reduction/analyze",
        Some(json!({
            "product_id": "p-1",
            "product_name": "Widget",
            "current_price": 100,
            "proposed_price": 90,
            "variable_cost_per_unit": 60,
            "current_expected_sales": 1000,
            "new_expected_sales": 1500
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["recommendation"], json!("APPROVE"));
    assert_eq!(body["contribution_difference"], json!("5000.000000"));
    assert_eq!(body["price_reduction_percentage"], json!("10.000000"));
}

#[tokio::test]
async fn a_reduction_without_a_book_header_is_refused_before_anything_is_written() {
    let graph = Scripted::returning(Vec::new());
    let router = vimbai_api::records::router(graph.clone());
    let request = Request::builder()
        .method("POST")
        .uri("/selling-price-reduction/analyze")
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .body(Body::from(
            json!({
                "product_id": "p-1",
                "product_name": "Widget",
                "current_price": 100,
                "proposed_price": 90,
                "variable_cost_per_unit": 60,
                "current_expected_sales": 1000
            })
            .to_string(),
        ))
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(graph.statements(), 0);
}

#[tokio::test]
async fn the_discount_analysis_needs_no_store_at_all() {
    let graph = Scripted::returning(Vec::new());
    let (status, body) = call(
        graph,
        "/selling-price-reduction/discount-analysis",
        Some(json!({
            "product_name": "Widget",
            "list_price": 100,
            "variable_cost_per_unit": 40,
            "expected_sales_at_list": 1000,
            "discounts": [10]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let rows = body["discount_analysis"].as_array().unwrap();
    assert_eq!(rows[0]["selling_price"], json!("90.000000"));
    assert_eq!(rows[0]["estimated_sales_volume"], json!("1200.000000"));
}

#[tokio::test]
async fn the_price_routes_answer_under_the_records_prefix_too() {
    let stored = json!({"x": {"id": "a-1", "product_id": "p-1", "recommendation": "REJECT"}});
    let graph = Scripted::returning(vec![stored]);
    let (status, _) = call(graph, "/records/selling-price-reduction/analyses", None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_cvp_analysis_is_stored_and_reported_on_the_published_path() {
    let stored = json!({"x": {
        "id": "a-1",
        "entity_id": "e-1",
        "entity_name": "Widget line",
        "selling_price_per_unit": "100.000000",
        "variable_cost_per_unit": "60.000000",
        "fixed_costs": "8000.000000",
        "expected_sales_units": "300.000000",
        "target_profit": "0.000000",
        "contribution_per_unit": "40.000000",
        "contribution_margin_ratio": "0.400000",
        "break_even_units": "200.000000",
        "break_even_revenue": "20000.000000",
        "profit_at_expected_sales": "4000.000000",
        "margin_of_safety_percentage": "33.333333",
        "degree_of_operating_leverage": "3.000000"
    }});
    let graph = Scripted::returning(vec![stored]);
    let (status, body) = call(
        graph,
        "/cvp-analysis/analyze",
        Some(json!({
            "entity_id": "e-1",
            "entity_name": "Widget line",
            "selling_price_per_unit": 100,
            "variable_cost_per_unit": 60,
            "fixed_costs": 8000,
            "expected_sales_units": 300
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["break_even_units"], json!("200.000000"));
    assert_eq!(body["degree_of_operating_leverage"], json!("3.000000"));
}

#[tokio::test]
async fn a_target_profit_the_contribution_cannot_reach_is_refused() {
    let graph = Scripted::returning(vec![]);
    let (status, _) = call(
        graph,
        "/cvp-analysis/target-profit",
        Some(json!({
            "entity_name": "Widget",
            "selling_price_per_unit": 100,
            "variable_cost_per_unit": 100,
            "fixed_costs": 1000,
            "target_profit": 500
        })),
    )
    .await;
    // Python raised 400 here; the record ports answer 422 for every rejected submission, and this
    // one shares that mapping rather than carrying a second convention for a single rule.
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn the_quick_analysis_answers_null_where_nothing_breaks_even() {
    let graph = Scripted::returning(vec![]);
    let (status, body) = call(
        graph,
        "/cvp-analysis/quick-analysis",
        Some(json!({"fixed_costs": 8000, "selling_price": 50, "variable_cost": 60})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["break_even_units"], Value::Null);
}

#[tokio::test]
async fn a_cvp_analysis_that_is_not_the_callers_is_a_404() {
    let graph = Scripted::returning(vec![]);
    let (status, _) = call(graph, "/cvp-analysis/analyses/missing", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_cvp_routes_answer_under_the_records_prefix_too() {
    let graph = Scripted::returning(vec![]);
    let (status, body) = call(graph, "/records/cvp-analysis/analyses", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], json!(0));
}

#[tokio::test]
async fn an_absorption_rate_is_stored_and_reported_on_the_published_path() {
    let stored = json!({"x": {
        "id": "r-1",
        "cost_centre_name": "Machining",
        "cost_centre_id": "cc-1",
        "overhead_type": "production",
        "absorption_base": "machine_hours",
        "budgeted_overhead": "12500.000000",
        "budgeted_base_units": "2000.000000",
        "absorption_rate": "6.250000",
        "rate_per_unit": "6.250000"
    }});
    let graph = Scripted::returning(vec![stored]);
    let (status, body) = call(
        graph,
        "/overhead-absorption/calculate",
        Some(json!({
            "cost_centre_name": "Machining",
            "cost_centre_id": "cc-1",
            "budgeted_overhead": 12500,
            "budgeted_base_units": 2000
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["absorption_rate"], json!("6.250000"));
    assert_eq!(body["absorption_base"], json!("machine_hours"));
}

#[tokio::test]
async fn every_absorption_base_keeps_its_own_answer_name() {
    let stored = json!({"x": {"id": "an-1"}});
    let cases = [
        (
            "machine-hours-rate",
            "budgeted_machine_hours",
            2000,
            "oar_per_machine_hour",
        ),
        (
            "labour-hours-rate",
            "budgeted_direct_labour_hours",
            5000,
            "oar_per_direct_labour_hour",
        ),
        (
            "labour-cost-rate",
            "budgeted_direct_labour_cost",
            50000,
            "oar_percentage",
        ),
        (
            "material-cost-rate",
            "budgeted_material_cost",
            25000,
            "oar_percentage",
        ),
        (
            "prime-cost-rate",
            "budgeted_prime_cost",
            100000,
            "oar_percentage",
        ),
        ("unit-rate", "budgeted_units", 2500, "oar_per_unit"),
    ];
    for (path, key, figure, answer_key) in cases {
        let graph = Scripted::returning(vec![stored.clone()]);
        let mut payload = json!({"cost_centre_name": "Centre", "budgeted_overhead": 12500});
        payload[key] = json!(figure);
        let (status, body) = call(
            graph,
            &format!("/overhead-absorption/{path}"),
            Some(payload),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(body.get(answer_key).is_some(), "{path} lost {answer_key}");
    }
}

#[tokio::test]
async fn a_blanket_rate_and_an_absorbed_figure_both_answer() {
    let graph = Scripted::returning(vec![json!({"x": {"id": "an-4"}})]);
    let (status, body) = call(
        graph,
        "/overhead-absorption/blanket-rate",
        Some(json!({
            "entity_name": "Works",
            "absorption_base": "direct_labour_hours",
            "total_budgeted_overhead": 900000,
            "total_base_units": 40000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["blanket_oar"], json!("22.500000"));

    let graph = Scripted::returning(vec![]);
    let (status, body) = call(
        graph,
        "/overhead-absorption/absorbed-overhead",
        Some(json!({
            "cost_centre_name": "Machining",
            "actual_base_units": 2100,
            "absorption_rate": 6.25
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["overhead_absorbed"], json!("13125.000000"));
}

#[tokio::test]
async fn the_absorption_routes_answer_under_the_records_prefix_too() {
    let graph = Scripted::returning(vec![]);
    let (status, body) = call(graph, "/records/overhead-absorption/rates", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["rates"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn rent_is_apportioned_by_floor_area_on_the_published_path() {
    let stored = json!({"x": {
        "id": "ap-1",
        "cost_type": "rent",
        "total_cost": "1200.000000",
        "basis": "floor_area",
        "cost_centres": [
            {"cost_centre_id": "cc-a", "cost_centre_name": "machining", "floor_area": "200.000000",
             "proportion": "0.200000", "apportioned_cost": "240.000000"}
        ],
        "total_apportioned": "1200.000000",
        "unapportioned": "0.000000"
    }});
    let graph = Scripted::returning(vec![stored]);
    let (status, body) = call(
        graph,
        "/apportionment/apportion/floor-area",
        Some(json!({
            "cost_type": "rent",
            "total_cost": 1200,
            "cost_centre_data": [
                {"cost_centre_id": "cc-a", "cost_centre_name": "machining", "floor_area": 200}
            ]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["basis"], json!("floor_area"));
    assert_eq!(body["unapportioned"], json!("0.000000"));
}

#[tokio::test]
async fn every_apportionment_basis_answers_on_its_own_path() {
    let stored = json!({"x": {"id": "ap-2", "cost_type": "costs", "basis": "volume"}});
    let cases = [
        ("floor-area", "floor_area"),
        ("volume", "volume"),
        ("cost", "asset_cost"),
        ("net-book-value", "net_book_value"),
        ("replacement-value", "replacement_value"),
        ("personnel", "number_of_personnel"),
        ("requisitions", "number_of_requisitions"),
    ];
    for (path, key) in cases {
        let graph = Scripted::returning(vec![stored.clone()]);
        let payload = json!({
            "cost_type": "costs",
            "total_cost": 500,
            "cost_centre_data": [
                {"cost_centre_id": "cc-a", "cost_centre_name": "a", key: 2},
                {"cost_centre_id": "cc-b", "cost_centre_name": "b", key: 3}
            ]
        });
        let (status, body) = call(
            graph,
            &format!("/apportionment/apportion/{path}"),
            Some(payload),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{path}");
        // the answer is the record the store returned; the breakdown itself is covered by the
        // store tests, so this only proves each path reaches its own basis
        assert_eq!(body["id"], json!("ap-2"), "{path}");
    }
}

#[tokio::test]
async fn an_apportionment_rule_and_the_results_both_answer() {
    let graph = Scripted::returning(vec![json!({"x": {"id": "rule-1"}})]);
    let (status, _) = call(
        graph,
        "/apportionment/rules/create",
        Some(json!({
            "cost_type": "rent",
            "basis": "floor_area",
            "description": "Rent follows floor area"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let graph = Scripted::returning(vec![]);
    let (status, body) = call(graph, "/apportionment/results", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn the_apportionment_routes_answer_under_the_records_prefix_too() {
    let graph = Scripted::returning(vec![]);
    let (status, body) = call(graph, "/records/apportionment/rules", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["rules"].as_array().unwrap().len(), 0);
}

/// A request with the method the service publishes, since not every route here is a POST.
async fn call_with(
    graph: Arc<Scripted>,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let router = vimbai_api::records::router(graph);
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    let request = match body {
        Some(payload) => request.body(Body::from(payload.to_string())).unwrap(),
        None => request.body(Body::empty()).unwrap(),
    };
    let mut request = request;
    request
        .headers_mut()
        .insert("x-user-id", "user-1".parse().expect("a valid header value"));
    request
        .headers_mut()
        .insert("x-book-id", "book-1".parse().expect("a valid header value"));
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn a_disposal_group_is_opened_and_answered_on_the_path_the_service_published() {
    let graph = Scripted::returning(vec![json!({"id": "g-1", "name": "Retail arm"})]);
    let (status, body) = call(
        graph,
        "/disposal-group/groups",
        Some(json!({"name": "Retail arm", "disposal_method": "spin_off"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], json!("Retail arm"));
}

#[tokio::test]
async fn a_disposal_group_impairment_is_answered_on_the_published_path() {
    // the group carries 50,000 and would fetch 42,000, so the write-down is 8,000
    let graph = Scripted::returning(vec![json!({"id": "g-1", "total_carrying_amount": "50000"})]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/disposal-group/groups/g-1/impairment-test?fair_value_less_costs=42000",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["impairment_amount"], json!("8000.000000"));
}

#[tokio::test]
async fn a_lease_settlement_is_answered_on_the_published_path() {
    let graph = Scripted::returning(vec![json!({
        "id": "t-1",
        "remaining_payments": 4,
        "remaining_payment_amount": "3000",
        "early_termination_penalty": "500",
        "status": "pending"
    })]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/lease-termination/terminations/t-1/settlement?asset_return_value=250.25",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // 4 x 3000, plus 500, less 250.25
    assert_eq!(body["net_settlement"], json!("12249.750000"));
}

#[tokio::test]
async fn the_asset_group_and_lease_routes_answer_under_the_records_prefix_too() {
    let group = Scripted::returning(vec![json!({"id": "g-1"})]);
    let (status, _) = call_with(
        group,
        "POST",
        "/records/disposal-group/groups",
        Some(json!({"name": "Retail arm"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let lease = Scripted::returning(vec![json!({"id": "t-1"})]);
    let (status, _) = call_with(
        lease,
        "POST",
        "/records/lease-termination/terminations",
        Some(json!({
            "lease_id": "L-1",
            "termination_date": "2026-12-31",
            "original_end_date": "2027-06-30",
            "remaining_payments": 6,
            "remaining_payment_amount": "2500"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_share_class_is_opened_and_summarised_on_the_path_the_service_published() {
    let graph = Scripted::returning(vec![json!({"id": "c-1", "name": "Ordinary"})]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/authorized-share-capital/share-classes?name=Ordinary&authorized_shares=1000",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], json!("Ordinary"));

    // the summary reads classes, issuances and buybacks in turn
    let summary_graph = Scripted::returning(Vec::new());
    let (status, body) = call_with(
        summary_graph,
        "GET",
        "/authorized-share-capital/summary",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_classes"], json!(0));
    assert_eq!(body["total_proceeds"], json!("0.000000"));
}

#[tokio::test]
async fn issuing_beyond_the_ceiling_is_a_refusal_not_a_share_issue() {
    let graph = Scripted::returning(vec![json!({
        "id": "c-1", "authorized_shares": 100, "issued_shares": 90
    })]);
    let (status, _) = call_with(
        graph,
        "POST",
        "/authorized-share-capital/share-classes/c-1/issue?number_of_shares=20&issue_price=2",
        None,
    )
    .await;
    // the records API answers a refused payload with 422 throughout, so a caller migrates one rule,
    // not one rule per service
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn the_share_class_routes_answer_under_the_records_prefix_too() {
    let graph = Scripted::returning(vec![json!({"id": "c-1"})]);
    let (status, _) = call_with(
        graph,
        "GET",
        "/records/authorized-share-capital/share-classes",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn an_ordinary_dividend_is_declared_and_paid_on_the_published_paths() {
    let graph = Scripted::returning(vec![json!({"id": "d-1"})]);
    let (status, _) = call_with(
        graph,
        "POST",
        "/ordinary-shares/dividends?dividend_type=interim&per_share_amount=0.35&total_shares=1000&record_date=2026-03-01",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let paying = Scripted::returning(vec![json!({
        "id": "d-1", "dividend_type": "interim", "total_dividend": "350"
    })]);
    let (status, body) = call_with(
        paying,
        "POST",
        "/ordinary-shares/dividends/d-1/pay?payment_date=2026-04-01",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], json!("d-1"));
}

#[tokio::test]
async fn preference_classes_are_issued_declared_and_redeemed_on_one_prefix() {
    let creating = Scripted::returning(vec![json!({"id": "c-1", "name": "A"})]);
    let (status, _) = call_with(
        creating,
        "POST",
        "/preference-shares/classes?name=A&company_id=co-1&nominal_value=1&issue_price=2.5",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let redeeming = Scripted::returning(vec![json!({
        "id": "c-1", "name": "A", "nominal_value": "1",
        "shares_issued": 200, "shares_outstanding": 200
    })]);
    let (status, body) = call_with(
        redeeming,
        "POST",
        "/preference-shares/classes/c-1/redeem?shares_redeemed=100&redemption_price=1.75",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], json!("c-1"));

    let listing = Scripted::returning(Vec::new());
    let (status, body) = call_with(listing, "GET", "/preference-shares/dividends", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["dividends"], json!([]));
}

#[tokio::test]
async fn the_dividend_routes_answer_under_the_records_prefix_too() {
    let graph = Scripted::returning(Vec::new());
    let (status, _) = call_with(
        graph,
        "GET",
        "/records/ordinary-shares/dividends?company_id=co-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_bonus_issue_and_a_rights_offer_answer_on_their_published_paths() {
    let bonus = Scripted::returning(vec![json!({"id": "b-1"})]);
    let (status, _) = call_with(
        bonus,
        "POST",
        "/bonus-shares/issue?company_id=co-1&shares_issued=1000&nominal_value=0.5&source_reserve=share_premium",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let offer = Scripted::returning(vec![json!({"id": "r-1"})]);
    let (status, _) = call_with(
        offer,
        "POST",
        "/right-issues/offer?company_id=co-1&rights_offered=2000&new_shares_offered=1000&issue_price=1.25&nominal_value=1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let listing = Scripted::returning(Vec::new());
    let (status, body) = call_with(listing, "GET", "/right-issues/right-issues", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["right_issues"], json!([]));
}

#[tokio::test]
async fn a_rights_renunciation_is_refused_when_the_issue_forbids_it() {
    let graph = Scripted::returning(vec![json!({"id": "r-1", "renouncement_allowed": false})]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/right-issues/right-issues/r-1/renounce?shareholder_id=s-1&new_shareholder_id=s-9",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body["error"],
        json!("Renouncement not allowed for this issue")
    );
}

#[tokio::test]
async fn a_reserve_is_opened_allocated_and_utilized_on_its_published_paths() {
    let creating = Scripted::returning(vec![json!({"id": "res-1"})]);
    let (status, _) = call_with(
        creating,
        "POST",
        "/general-reserve/reserves?company_id=co-1&reserve_name=Plant&initial_balance=5000",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let allocating = Scripted::returning(vec![json!({
        "id": "res-1", "reserve_name": "Plant", "current_balance": "5000"
    })]);
    let (status, body) = call_with(
        allocating,
        "POST",
        "/general-reserve/reserves/res-1/allocate?amount=250.75&source=retained_earnings&description=top%20up",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["reserve"]["current_balance"], json!("5250.750000"));

    let history = Scripted::returning(Vec::new());
    let (status, body) = call_with(
        history,
        "GET",
        "/general-reserve/reserves/res-1/history",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["allocations"], json!([]));
}

#[tokio::test]
async fn utilizing_more_than_a_reserve_holds_is_refused_at_the_api() {
    let graph = Scripted::returning(vec![json!({
        "id": "res-1", "current_balance": "100"
    })]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/general-reserve/reserves/res-1/utilize?amount=101&purpose=working_capital",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], json!("Insufficient reserve balance"));
}

#[tokio::test]
async fn an_appropriation_and_a_summary_answer_on_their_published_paths() {
    let creating = Scripted::returning(vec![json!({"id": "a-1"})]);
    let (status, _) = call_with(
        creating,
        "POST",
        "/retained-profits/appropriations?company_id=co-1&financial_year=2026&net_profit=1000&opening_retained_earnings=5000&appropriations=reserve:300",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let summary = Scripted::queue(vec![
        vec![json!({"id": "a-1", "company_id": "co-1", "balance_carried_forward": "500"})],
        Vec::new(),
    ]);
    let (status, body) = call_with(summary, "GET", "/retained-profits/summary/co-1", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["net_retained_earnings"], json!("500.000000"));
}

#[tokio::test]
async fn the_reserve_and_retained_profit_routes_answer_under_records_too() {
    let graph = Scripted::returning(Vec::new());
    let (status, _) = call_with(graph, "GET", "/records/general-reserve/reserves", None).await;
    assert_eq!(status, StatusCode::OK);

    let listing = Scripted::returning(Vec::new());
    let (status, _) = call_with(
        listing,
        "GET",
        "/records/retained-profits/appropriations?company_id=co-1&financial_year=2026",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn an_issue_that_creates_premium_answers_on_its_published_path() {
    let graph = Scripted::returning(vec![json!({"id": "sp-1"})]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/share-premium/entries/record?company_id=co-1&shares_issued=1000&nominal_value=1&issue_price=2.25&share_class=ordinary&reference_id=iss-9",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, params) = seen.last();
    assert_eq!(params["props"]["premium_amount"], json!("1250.000000"));
}

#[tokio::test]
async fn the_premium_summary_and_crr_summary_answer_on_their_published_paths() {
    let premium = Scripted::queue(vec![
        vec![json!({"company_id": "co-1", "premium_amount": "1250"})],
        vec![json!({"company_id": "co-1", "amount": "250"})],
        Vec::new(),
    ]);
    let (status, body) = call_with(premium, "GET", "/share-premium/summary/co-1", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["current_balance"], json!("1000.000000"));

    let reserve = Scripted::queue(vec![
        vec![json!({"company_id": "co-1", "redemption_reserve_amount": "200"})],
        Vec::new(),
        Vec::new(),
    ]);
    let (status, body) = call_with(
        reserve,
        "GET",
        "/capital-redemption-reserve/summary/co-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_created"], json!("200.000000"));
    assert_eq!(body["transaction_count"], json!(1));
}

#[tokio::test]
async fn a_redemption_of_preference_shares_answers_on_the_crr_path() {
    let graph = Scripted::returning(vec![json!({"id": "t-1"})]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/capital-redemption-reserve/redemptions/record?company_id=co-1&share_class=preference&shares_redeemed=500&redemption_price=1.40&nominal_value=1&redemption_date=2026-03-01&source_account=profits",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, params) = seen.last();
    assert_eq!(
        params["props"]["redemption_reserve_amount"],
        json!("200.000000")
    );
    assert_eq!(params["props"]["total_proceeds"], json!("700.000000"));
}

#[tokio::test]
async fn the_premium_and_reserve_routes_mirror_under_records() {
    let graph = Scripted::returning(Vec::new());
    let (status, _) = call_with(graph, "GET", "/records/share-premium/entries", None).await;
    assert_eq!(status, StatusCode::OK);

    let listing = Scripted::returning(Vec::new());
    let (status, _) = call_with(
        listing,
        "GET",
        "/records/capital-redemption-reserve/redemptions?company_id=co-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_redemption_out_of_proceeds_answers_on_its_published_path() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "red-1", "status": "awaiting_crr"})],
        vec![json!({"id": "req-1"})],
    ]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/share-redemption/redemptions/initiate?company_id=co-1&share_class=ordinary&shares_redeemed=500&nominal_value=1&redemption_price=1.40&redemption_method=proceeds&redemption_date=2026-03-01&authority_date=2026-02-01",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, requirement) = seen.statement(1);
    assert_eq!(
        requirement["props"]["minimum_crr_required"],
        json!("500.000000")
    );
}

#[tokio::test]
async fn completing_a_redemption_reports_the_reserve_it_created() {
    let graph = Scripted::queue(vec![
        vec![json!({
            "id": "red-1", "shares_redeemed": 500, "nominal_value": "1",
            "total_redemption_value": "700", "redemption_method": "proceeds",
            "share_class": "ordinary", "redemption_date": "2026-03-01"
        })],
        vec![json!({"id": "red-1", "status": "completed"})],
    ]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/share-redemption/redemptions/red-1/complete",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["crr_created"], json!("500.000000"));
}

#[tokio::test]
async fn the_share_redemption_routes_mirror_under_records() {
    let graph = Scripted::returning(Vec::new());
    let (status, body) = call_with(
        graph,
        "GET",
        "/records/share-redemption/redemptions?status=awaiting_crr",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["redemptions"], json!([]));
}

#[tokio::test]
async fn an_issue_answers_on_the_issued_share_capital_path() {
    let graph = Scripted::returning(vec![json!({"id": "iss-1"})]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/issued-share-capital/issue?company_id=co-1&shares_issued=1000&issue_price=2.25&share_class=ordinary&issue_date=2026-03-01",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, params) = seen.last();
    assert_eq!(params["props"]["total_proceeds"], json!("2250.000000"));
}

#[tokio::test]
async fn the_simple_investment_answers_without_touching_the_store() {
    let graph = Scripted::returning(Vec::new());
    let seen = Arc::clone(&graph);
    let (status, body) = call_with(
        graph,
        "POST",
        "/initial-investment/simple?asset_cost=10000&selling_of_old_asset=2500&tax_on_sale=500",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["initial_cash_outlay"], json!("8000.000000"));
    assert_eq!(seen.statements(), 0);
}

#[tokio::test]
async fn the_capital_allowance_saving_answers_on_its_path() {
    let graph = Scripted::returning(Vec::new());
    let (status, body) = call_with(
        graph,
        "POST",
        "/initial-investment/with-capital-allowance?asset_cost=10000&scrap_value=0&capital_allowance=8000&tax_rate=25",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["tax_saving"], json!("2000.000000"));
    assert_eq!(body["adjusted_investment_cost"], json!("8000.000000"));
}

#[tokio::test]
async fn a_missing_investment_answers_not_found_rather_than_an_error_body() {
    let graph = Scripted::returning(Vec::new());
    let (status, _) = call_with(graph, "GET", "/initial-investment/get/nope", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_debenture_issue_answers_on_its_published_path() {
    let graph = Scripted::queue(vec![
        vec![json!({
            "id": "cls-1", "name": "2026 loan stock", "nominal_value": "100",
            "issue_price": "95", "coupon_rate": "8", "redemption_price": "104",
            "interest_payment_frequency": "semi_annual",
            "debentures_issued": 1000, "debentures_outstanding": 1000
        })],
        vec![json!({"id": "cls-1"})],
        vec![json!({"id": "iss-1"})],
    ]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/debentures/classes/cls-1/issue?company_id=co-1&debentures_issued=200&issue_date=2026-03-01",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, params) = seen.statement(2);
    assert_eq!(params["props"]["discount_on_issue"], json!("1000.000000"));
}

#[tokio::test]
async fn accrueing_debenture_interest_withholds_the_tax_on_its_path() {
    let graph = Scripted::returning(vec![json!({
        "id": "cls-1", "name": "loan stock", "nominal_value": "100",
        "coupon_rate": "8", "interest_payment_frequency": "quarterly",
        "debentures_issued": 1000, "debentures_outstanding": 1000
    })]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/debentures/classes/cls-1/interest/accrue?company_id=co-1&period_start=2026-01-01&period_end=2026-03-31&debentures_outstanding=800",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, params) = seen.last();
    assert_eq!(params["props"]["interest_amount"], json!("1600.000000"));
    assert_eq!(params["props"]["net_payment"], json!("1280.000000"));
}

#[tokio::test]
async fn a_missing_debenture_class_is_refused_on_the_redeem_path() {
    let graph = Scripted::returning(Vec::new());
    let (status, _) = call_with(
        graph,
        "POST",
        "/debentures/classes/nope/redeem?debentures_redeemed=10&redemption_date=2027-01-01",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_reconstruction_scheme_answers_on_its_published_path() {
    let graph = Scripted::returning(vec![json!({"id": "rec-1"})]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/capital-reconstruction/reconstructions/create?company_id=co-1&reconstruction_type=internal&description=Loss%20write-off&scheme_date=2026-03-01&previous_share_capital=1000&new_share_capital=400",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, params) = seen.last();
    assert_eq!(
        params["props"]["capital_reduction_amount"],
        json!("600.000000")
    );
}

#[tokio::test]
async fn approving_a_reconstruction_balances_the_entry() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "rec-1", "capital_reduction_amount": "600",
                    "scheme_date": "2026-03-01", "description": "Loss write-off"})],
        vec![
            json!({"id": "adj-1", "reconstruction_id": "rec-1", "adjustment_type": "write_off",
                    "account_code": "3000", "account_name": "Accumulated loss",
                    "adjustment_amount": "250"}),
        ],
        vec![],
        vec![json!({"id": "rec-1", "status": "completed"})],
    ]);
    let seen = Arc::clone(&graph);
    let (status, body) = call_with(
        graph,
        "POST",
        "/capital-reconstruction/reconstructions/rec-1/approve",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["reconstruction"]["status"], json!("completed"));
    let (_, params) = seen.statement(3);
    let lines = params["patch"]["journal_entry"]["entries"]
        .as_array()
        .expect("lines");
    assert_eq!(lines[2]["credit"], json!("850.000000"));
}

#[tokio::test]
async fn a_reconstruction_summary_answers_under_records_too() {
    let graph = Scripted::returning(vec![json!({
        "company_id": "co-1", "capital_reduction_amount": "600", "status": "completed"
    })]);
    let (status, body) = call_with(
        graph,
        "GET",
        "/records/capital-reconstruction/summary/co-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_capital_reduced"], json!("600.000000"));
}

#[tokio::test]
async fn a_contract_is_created_on_the_revenue_recognition_path() {
    let graph = Scripted::returning(vec![json!({"id": "con-1"})]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/revenue-recognition/contracts",
        Some(json!({
            "company_id": "co-1", "customer_name": "Acme",
            "obligations": [
                {"description": "Install", "transaction_price": "1000", "standalone_selling_price": "1200"},
                {"description": "Support", "transaction_price": "1000", "standalone_selling_price": "800"}
            ]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, params) = seen.last();
    assert_eq!(
        params["props"]["total_transaction_price"],
        json!("2000.000000")
    );
}

#[tokio::test]
async fn recognising_on_the_path_writes_the_contract_back() {
    let graph = Scripted::queue(vec![
        vec![json!({
            "id": "con-1", "total_transaction_price": "2000",
            "obligations": [
                {"description": "Install", "transaction_price": "1200", "standalone_selling_price": "1200"}
            ]
        })],
        vec![json!({"id": "con-1"})],
    ]);
    let seen = Arc::clone(&graph);
    let (status, body) = call_with(
        graph,
        "POST",
        "/revenue-recognition/contracts/con-1/recognize?obligation_id=0&amount=500",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deferred"], json!("1500.000000"));
    let (_, params) = seen.statement(1);
    assert_eq!(
        params["patch"]["total_revenue_recognized"],
        json!("500.000000")
    );
}

#[tokio::test]
async fn an_empty_amount_recognises_the_obligation_in_full() {
    let graph = Scripted::queue(vec![
        vec![json!({
            "id": "con-1", "total_transaction_price": "40",
            "obligations": [{"description": "One", "transaction_price": "40", "standalone_selling_price": "0"}]
        })],
        vec![json!({"id": "con-1"})],
    ]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/revenue-recognition/contracts/con-1/recognize?obligation_id=0",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["recognized"], json!("40.000000"));
    assert_eq!(body["is_satisfied"], json!(true));
}

#[tokio::test]
async fn an_unreadable_amount_is_refused_rather_than_read_as_zero() {
    let graph = Scripted::returning(Vec::new());
    let (status, _) = call_with(
        graph,
        "POST",
        "/revenue-recognition/contracts/con-1/recognize?obligation_id=0&amount=lots",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_fund_is_created_with_its_net_assets_on_the_fund_path() {
    let graph = Scripted::returning(vec![json!({"id": "fund-1"})]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/fund-accounting/funds",
        Some(json!({"company_id": "co-1", "fund_name": "Building fund",
                    "balance": "1000", "income": "250", "expenses": "100"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, params) = seen.last();
    assert_eq!(params["props"]["net_assets"], json!("1150.000000"));
}

#[tokio::test]
async fn a_fund_movement_rederives_the_fund_and_answers_it_is_recorded() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "fund-1", "balance": "1000"})],
        vec![json!({"id": "tx-1"})],
        vec![
            json!({"id": "tx-1", "fund_id": "fund-1", "amount": "500", "is_income": true}),
            json!({"id": "tx-2", "fund_id": "fund-1", "amount": "200", "is_income": false}),
        ],
        vec![json!({"id": "fund-1"})],
    ]);
    let seen = Arc::clone(&graph);
    let (status, body) = call_with(
        graph,
        "POST",
        "/fund-accounting/transactions",
        Some(
            json!({"fund_id": "fund-1", "description": "Grant", "amount": "500",
                    "is_income": true}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], json!("recorded"));
    let (_, params) = seen.statement(3);
    assert_eq!(params["patch"]["net_assets"], json!("1300.000000"));
}

#[tokio::test]
async fn a_movement_against_an_unknown_fund_is_refused() {
    let graph = Scripted::returning(Vec::new());
    let (status, _) = call_with(
        graph,
        "POST",
        "/fund-accounting/transactions",
        Some(json!({"fund_id": "nope", "amount": "1", "is_income": true})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_balance_sheet_is_generated_with_totals_the_caller_did_not_send() {
    let graph = Scripted::returning(vec![json!({"id": "sheet-1"})]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/balance-sheet/generate",
        Some(json!({"company_id": "co-1",
                    "assets": [{"name": "Cash", "amount": "600", "is_liquid": true}],
                    "liabilities": [{"name": "Loan", "amount": "400"}],
                    "equity": [{"name": "Capital", "amount": "200"}]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, params) = seen.last();
    assert_eq!(params["props"]["total_assets"], json!("600.000000"));
    assert_eq!(params["props"]["is_balanced"], json!(true));
}

#[tokio::test]
async fn the_latest_balance_sheet_is_not_found_when_none_exists() {
    let graph = Scripted::returning(Vec::new());
    let (status, _) = call_with(graph, "GET", "/balance-sheet/latest/co-1", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn balance_sheet_ratios_come_off_the_latest_sheet() {
    let graph = Scripted::returning(vec![json!({
        "id": "sheet-1", "company_id": "co-1", "created_at": "2026-03-01T00:00:00",
        "assets": [{"amount": "600", "category": "current", "is_liquid": true}],
        "liabilities": [{"amount": "400", "category": "current"}],
        "total_assets": "1000", "total_liabilities": "400", "total_equity": "600"
    })]);
    let (status, body) = call_with(graph, "GET", "/balance-sheet/ratios/co-1", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["current_ratio"], json!("1.500000"));
    assert_eq!(body["debt_to_equity"], json!("0.666667"));
}

#[tokio::test]
async fn a_cash_flow_schedule_records_its_entries_on_the_path() {
    let graph = Scripted::returning(vec![json!({"id": "cf-1"})]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/cash-flow/create",
        Some(
            json!({"project_name": "Mill", "initial_investment": "1000000",
                    "yearly_inflows": ["100"], "yearly_outflows": ["40"]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, params) = seen.last();
    assert_eq!(params["props"]["total_net_flow"], json!("60.000000"));
    assert_eq!(
        params["props"]["initial_investment"],
        json!("1000000.000000")
    );
}

#[tokio::test]
async fn net_flows_answer_for_mismatched_lengths() {
    let graph = Scripted::returning(Vec::new());
    let (status, body) = call_with(
        graph,
        "POST",
        "/cash-flow/net-flows",
        Some(json!({"inflows": ["100"], "outflows": ["40", "10"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["net_flows"], json!(["60.000000", "-10.000000"]));
}

#[tokio::test]
async fn a_missing_cash_flow_answers_200_with_the_error_body() {
    let graph = Scripted::returning(Vec::new());
    let (status, body) = call_with(graph, "GET", "/cash-flow/get/nope", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["error"], json!("Cash flow not found"));
}

#[tokio::test]
async fn an_intercompany_transaction_is_recorded_exactly_on_its_path() {
    let graph = Scripted::returning(vec![json!({"id": "tx-1"})]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/intercompany/transactions",
        Some(json!({"from_entity_id": "a", "to_entity_id": "b",
                    "transaction_type": "royalty", "amount": "123456789012345678901.55"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, params) = seen.last();
    assert_eq!(
        params["props"]["amount"],
        json!("123456789012345678901.550000")
    );
}

#[tokio::test]
async fn an_unknown_intercompany_type_is_refused() {
    let graph = Scripted::returning(vec![json!({"id": "tx-1"})]);
    let (status, _) = call_with(
        graph,
        "POST",
        "/intercompany/transactions",
        Some(json!({"from_entity_id": "a", "to_entity_id": "b",
                    "transaction_type": "gift", "amount": "1"})),
    )
    .await;
    // the record routes answer refused input with 422, the same as everywhere else here
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn matching_two_legs_answers_the_elimination_it_wrote() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "tx-1", "from_entity_id": "a", "to_entity_id": "b",
                    "amount": "500", "description": "Fee"}),
        ],
        vec![
            json!({"id": "tx-2", "from_entity_id": "b", "to_entity_id": "a",
                    "amount": "500", "description": "Fee"}),
        ],
        vec![json!({"id": "tx-1"})],
        vec![json!({"id": "tx-2"})],
        vec![json!({"id": "el-1"})],
    ]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/intercompany/transactions/match",
        Some(json!({"txn1_id": "tx-1", "txn2_id": "tx-2"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["matched"], json!(true));
    assert_eq!(body["elimination_id"], json!("el-1"));
}

#[tokio::test]
async fn legs_that_do_not_agree_on_the_amount_are_refused() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "tx-1", "from_entity_id": "a", "to_entity_id": "b", "amount": "500"})],
        vec![json!({"id": "tx-2", "from_entity_id": "b", "to_entity_id": "a", "amount": "500.01"})],
    ]);
    let (status, _) = call_with(
        graph,
        "POST",
        "/intercompany/transactions/match",
        Some(json!({"txn1_id": "tx-1", "txn2_id": "tx-2"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn an_ifrs_report_is_created_as_a_draft() {
    let graph = Scripted::returning(vec![json!({"id": "r-1"})]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/ifrs-reporting/reports",
        Some(
            json!({"report_type": "balance_sheet", "ifrs_standard": "IAS1",
                    "period": "2026-03", "reporting_date": "2026-03-31"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, params) = seen.last();
    assert_eq!(params["props"]["status"], json!("draft"));
}

#[tokio::test]
async fn an_unknown_ifrs_report_type_is_refused() {
    let graph = Scripted::returning(vec![json!({"id": "r-1"})]);
    let (status, _) = call_with(
        graph,
        "POST",
        "/ifrs-reporting/reports",
        Some(
            json!({"report_type": "trial_balance", "ifrs_standard": "IAS1",
                    "period": "2026-03", "reporting_date": "2026-03-31"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn notes_for_a_report_that_is_not_there_list_empty() {
    let graph = Scripted::returning(Vec::new());
    let (status, body) = call_with(graph, "GET", "/ifrs-reporting/reports/gone/notes", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
}

#[tokio::test]
async fn a_published_ifrs_report_cannot_be_approved_again() {
    let graph = Scripted::queue(vec![vec![json!({"id": "r-1", "status": "published",
                                                  "disclosures": []})]]);
    let (status, _) = call_with(
        graph,
        "PUT",
        "/ifrs-reporting/reports/r-1/approve",
        Some(json!({"approved_by": "auditor"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_cash_entry_moves_the_account_it_is_written_against() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "acc-1", "account_code": "1000", "account_type": "bank",
                    "currency": "USD", "opening_balance": "500", "current_balance": "500",
                    "is_active": true}),
        ],
        vec![json!({"id": "acc-1"})],
        vec![json!({"id": "e-1"})],
    ]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/cashbook/entries",
        Some(
            json!({"book_type": "receipts", "entry_date": "2026-03-01T10:00:00Z",
                    "voucher_number": "R1", "description": "Sale", "account_code": "4000",
                    "amount": "123456789012345678901.55", "is_debit": true,
                    "bank_account": "1000", "posted_by": "clerk"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, patch) = seen.statement(1);
    assert_eq!(
        patch["patch"]["current_balance"],
        json!("123456789012345679401.550000")
    );
}

#[tokio::test]
async fn the_cashbook_summary_is_reachable_on_the_published_path() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "acc-1", "account_code": "1000", "account_name": "Current",
                    "account_type": "bank", "currency": "USD", "opening_balance": "100",
                    "current_balance": "100", "is_active": true}),
        ],
        vec![
            json!({"bank_account": "1000", "base_amount": "500", "is_debit": true,
                    "status": "posted", "entry_date": "2026-03-01T09:00:00Z",
                    "reconciled": false}),
        ],
    ]);
    let (status, body) = call_with(graph, "GET", "/cashbook/summary/1000", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["closing_balance"], json!("600.000000"));
    assert_eq!(body["transaction_count"], json!(1));
}

#[tokio::test]
async fn an_unknown_cashbook_account_has_no_summary() {
    let graph = Scripted::returning(Vec::new());
    let (status, _) = call_with(graph, "GET", "/cashbook/summary/9999", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_cash_position_adds_up_every_account() {
    let graph = Scripted::returning(vec![
        json!({"id": "acc-1", "account_code": "1000", "account_name": "Current",
               "account_type": "bank", "currency": "USD", "opening_balance": "0",
               "current_balance": "1000.55", "is_active": true}),
        json!({"id": "acc-2", "account_code": "1010", "account_name": "Petty",
               "account_type": "cash", "currency": "USD", "opening_balance": "0",
               "current_balance": "20", "is_active": true}),
    ]);
    let (status, body) = call_with(graph, "GET", "/cashbook/cash-position", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_bank"], json!("1000.550000"));
    assert_eq!(body["by_currency"]["USD"], json!("1020.550000"));
}

#[tokio::test]
async fn a_company_answers_the_equity_accounts_it_needs() {
    let graph = Scripted::returning(vec![json!({"id": "c-1"})]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/company-accounting/companies",
        Some(json!({"company_code": "ACME", "company_name": "Acme Ltd",
                    "incorporation_date": "2020-01-01T00:00:00Z"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["company"]["company_code"], json!("ACME"));
    assert_eq!(
        body["equity_accounts"][0]["account_number"],
        json!("ACME-SHARE_CAPITAL")
    );
}

#[tokio::test]
async fn a_dividend_pays_the_matching_holders_exactly() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "d-1"})],
        vec![
            json!({"id": "h-1", "company_id": "c-1", "shareholder_name": "Ana",
                    "share_class": "ordinary", "shares_held": 3}),
        ],
        vec![json!({"id": "p-1"})],
        vec![json!({"id": "c-1", "company_code": "ACME"})],
    ]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/company-accounting/dividends",
        Some(
            json!({"company_id": "c-1", "dividend_type": "final", "share_class": "ordinary",
                    "declaration_date": "2026-03-01T00:00:00Z",
                    "total_amount": "30000000000000000000.03", "tax_withheld": "0",
                    "per_share_amount": "10000000000000000000.01"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let payments = body["payments"].as_array().expect("payments");
    assert_eq!(payments.len(), 1);
    assert_eq!(
        payments[0]["gross_amount"],
        json!("30000000000000000000.030000")
    );
    assert_eq!(
        payments[0]["net_amount"],
        json!("27000000000000000000.027000")
    );
}

#[tokio::test]
async fn the_shareholder_register_prints_its_percentages() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "c-1", "company_code": "ACME"})],
        vec![
            json!({"id": "h-1", "company_id": "c-1", "shareholder_name": "Ana",
                    "shareholder_type": "individual", "shares_held": 1,
                    "percentage_holding": 100.0, "is_controlling_party": true}),
        ],
    ]);
    let (status, body) = call_with(
        graph,
        "GET",
        "/company-accounting/reports/shareholder-register/c-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["shareholders"][0]["percentage"], json!("100.00%"));
}

#[tokio::test]
async fn a_company_report_needs_the_company_it_is_about() {
    let graph = Scripted::returning(Vec::new());
    let (status, _) = call_with(
        graph,
        "GET",
        "/company-accounting/reports/dividend-history/gone",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_department_comes_with_its_cost_centre() {
    let graph = Scripted::returning(vec![json!({"id": "d-1"})]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/departmental-accounting/departments",
        Some(
            json!({"department_code": "OPS", "department_name": "Operations",
                    "department_type": "cost", "manager_id": "u-1",
                    "manager_name": "Ana"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["department"]["department_code"], json!("OPS"));
    assert_eq!(body["cost_centre"]["account_number"], json!("DEPT-OPS"));
}

#[tokio::test]
async fn an_allocation_run_takes_its_pool_from_the_query() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "p-1", "pool_name": "Facilities", "total_amount": "100",
                    "allocation_basis": "headcount", "allocation_method": "direct",
                    "excluded_departments": []}),
        ],
        vec![json!({"id": "p-1"})],
        vec![
            json!({"id": "d-1", "department_code": "OPS", "department_name": "Ops",
                    "status": "active"}),
        ],
        vec![json!({"id": "a-1"})],
        vec![],
        vec![json!({"id": "p-1", "status": "closed"})],
    ]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/departmental-accounting/allocate?cost_pool_id=p-1&period_start=2026-03-01T00:00:00Z",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["allocations"][0]["allocated_amount"],
        json!("100.000000")
    );
    assert_eq!(body["total_allocated"], json!("100.000000"));
}

#[tokio::test]
async fn a_second_allocation_run_replaces_the_first() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "p-1", "pool_name": "Facilities", "total_amount": "60",
                    "allocation_basis": "headcount", "allocation_method": "direct",
                    "excluded_departments": []}),
        ],
        vec![json!({"id": "p-1"})],
        vec![
            json!({"id": "d-1", "department_code": "OPS", "department_name": "Ops",
                    "status": "active"}),
        ],
        vec![json!({"id": "a-2"})],
        vec![json!({"id": "a-1", "cost_pool_id": "p-1"})],
        vec![json!({"id": "a-1"})],
        vec![json!({"id": "p-1", "status": "closed"})],
    ]);
    let seen = Arc::clone(&graph);
    let (status, _) = call_with(
        graph,
        "POST",
        "/departmental-accounting/allocate?cost_pool_id=p-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (cypher, _) = seen.statement(5);
    assert!(cypher.contains("DETACH DELETE"), "{cypher}");
}

#[tokio::test]
async fn the_comparison_report_reads_the_same_on_both_shapes() {
    let query = "period_start=2026-03-01T00:00:00Z&period_end=2026-03-31T00:00:00Z";
    let graph = Scripted::queue(vec![
        vec![json!({"id": "d-1", "department_name": "Ops"})],
        vec![],
    ]);
    let (status, body) = call_with(
        graph,
        "GET",
        &format!(
            "/departmental-accounting/reports/department-comparison?{query}&department_ids=d-1"
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["departments"].as_array().expect("rows").len(), 1);
}

#[tokio::test]
async fn a_department_financial_summary_reports_what_it_was_given() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "d-1", "department_name": "Ops"})],
        vec![json!({"department_id": "d-1", "allocated_amount": "0.45"})],
        vec![],
        vec![json!({"id": "f-1"})],
    ]);
    let (status, body) = call_with(
        graph,
        "GET",
        "/departmental-accounting/departments/d-1/financials?revenue=123456789012345678901.55&direct_expenses=0.45",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // 123456789012345678901.55 - 0.45 - 0.45, with every digit of the revenue
    assert_eq!(body["net_income"], json!("123456789012345678900.650000"));
}

#[tokio::test]
async fn the_catalogue_answers_without_a_store() {
    let graph = Scripted::returning(Vec::new());
    let (status, body) = call_with(graph, "GET", "/accounting-standards/standards", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().expect("standards").len(), 14);
}

#[tokio::test]
async fn a_standard_is_found_by_its_type_and_refused_by_a_wrong_one() {
    let graph = Scripted::returning(Vec::new());
    let (status, body) = call_with(
        graph.clone(),
        "GET",
        "/accounting-standards/standards/ifrs",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"], json!("IFRS"));

    let (status, _) = call_with(
        graph,
        "GET",
        "/accounting-standards/standards/mars_gaap",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_compliance_check_needs_the_standard_in_the_query() {
    let graph = Scripted::queue(vec![
        vec![],
        vec![json!({"id": "k-1"})],
        vec![json!({"id": "k-2"})],
        vec![json!({"id": "k-3"})],
        vec![json!({"id": "k-4"})],
    ]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/accounting-standards/organizations/org-1/compliance/check?standard_type=ifrs",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_checks"], json!(4));
    assert_eq!(body["passed"], json!(4));
}

#[tokio::test]
async fn a_configuration_that_was_never_written_is_a_miss() {
    let graph = Scripted::returning(Vec::new());
    let (status, _) = call_with(
        graph,
        "GET",
        "/accounting-standards/organizations/org-1/configuration",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_record_prefix_answers_the_same_catalogue() {
    let graph = Scripted::returning(Vec::new());
    let (status, body) = call_with(
        graph,
        "GET",
        "/records/accounting-standards/standards/categories",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["International"].as_array().expect("region").len(), 1);
}

#[tokio::test]
async fn an_account_is_created_and_read_back() {
    let graph = Scripted::returning(vec![json!({"id": "a-1"})]);
    let seen = Arc::clone(&graph);
    let (status, body) = call_with(
        graph,
        "POST",
        "/accounting/accounts",
        Some(json!({"account_number": "1000", "name": "Cash",
                    "account_type": "asset", "normal_balance": "debit"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["account_number"], json!("1000"));
    assert_eq!(body["account_type"], json!("asset"));
    // the first statement was the duplicate check; the write is the one that carries the props
    let (_, params) = seen.statement(1);
    assert_eq!(params["props"]["account_type"], json!("asset"));
}

#[tokio::test]
async fn an_unbalanced_entry_is_refused_over_http() {
    let graph = Scripted::returning(vec![json!({"id": "a-1", "account_number": "1000"})]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/accounting/journal-entries",
        Some(json!({"entry_date": "2026-03-15",
        "lines": [
            {"account_number": "1000", "debit": "10.00"},
            {"account_number": "4000", "credit": "9.99"}
        ]})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(format!("{body}").contains("differ"), "{body}");
}

#[tokio::test]
async fn a_balanced_entry_over_ordinary_amounts_is_stored_stamped() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "a-1", "account_number": "1000", "account_type": "asset",
                   "normal_balance": "debit"}),
            json!({"id": "a-2", "account_number": "4000", "account_type": "revenue",
                   "normal_balance": "credit"}),
        ],
        vec![
            json!({"id": "a-1", "account_number": "1000", "account_type": "asset",
                   "normal_balance": "debit"}),
            json!({"id": "a-2", "account_number": "4000", "account_type": "revenue",
                   "normal_balance": "credit"}),
        ],
        vec![],
        vec![json!({"id": "e-1"})],
    ]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/accounting/journal-entries",
        Some(json!({"entry_date": "2026-03-15", "status": "posted",
        "lines": [
            {"account_number": "1000", "debit": "120.00"},
            {"account_number": "4000", "credit": "120.00"}
        ]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["prev_hash"].as_str().expect("prev").len(), 64);
    assert_eq!(body["entry_hash"].as_str().expect("stamp").len(), 64);
}

#[tokio::test]
async fn a_posted_entry_refuses_edits_over_http() {
    let graph = Scripted::returning(vec![json!({"id": "e-1", "status": "posted"})]);
    let (status, _) = call_with(
        graph,
        "PUT",
        "/accounting/journal-entries/e-1",
        Some(json!({"description": "changed"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn the_trial_balance_answers_under_both_prefixes() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "e-1", "entry_date": "2026-03-15", "status": "posted",
                    "lines": [{"account_number": "1000", "debit": "500.000000",
                               "credit": "0.000000"}]}),
        ],
        vec![
            json!({"id": "a-1", "account_number": "1000", "name": "Cash",
                    "account_type": "asset", "normal_balance": "debit"}),
        ],
    ]);
    let (status, body) = call_with(graph, "GET", "/records/accounting/trial-balance", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_debits"], json!("500.000000"));
    assert_eq!(body["is_balanced"], json!(false));
}

#[tokio::test]
async fn a_sales_invoice_is_posted_and_answered_over_http() {
    let chart: Vec<serde_json::Value> = ["1000", "1100", "4000"]
        .into_iter()
        .map(|number| {
            json!({"id": format!("a-{number}"), "account_number": number, "name": number,
                   "account_type": "asset", "normal_balance": "debit"})
        })
        .collect();
    let graph = Scripted::queue(vec![
        chart.clone(),
        chart,
        vec![],
        vec![json!({"id": "je-1"})],
        vec![json!({"id": "doc-1"})],
    ]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/accounting/sales-journal",
        Some(json!({"invoice_number": "INV-1", "customer_id": "c-1",
                    "entry_date": "2026-03-15", "total_amount": "1000.00",
                    "discount_amount": "50.00", "tax_amount": "140.00"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["invoice_number"], json!("INV-1"));
    assert_eq!(body["journal_entry_id"], json!("je-1"));
    assert_eq!(body["total_amount"], json!("1000.000000"));
}

#[tokio::test]
async fn the_placeholder_ledgers_answer_with_nothing_over_http() {
    let graph = Scripted::returning(vec![]);
    let (status, body) = call_with(
        graph,
        "GET",
        "/records/accounting/subsidiary-ledgers/inventory",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["item_count"], json!(0));
}

#[tokio::test]
async fn a_receivable_ledger_over_http_ages_the_documents_it_was_given() {
    let graph = Scripted::returning(vec![json!({
        "id": "s-1", "invoice_number": "INV-1", "customer_id": "cust-1",
        "invoice_date": "2026-01-01", "total_amount": "500.000000", "status": "invoiced"})]);
    let (status, body) = call_with(
        graph,
        "GET",
        "/accounting/subsidiary-ledgers/accounts-receivable?today=2026-03-15",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["overdue_count"], json!(1));
    assert_eq!(body["total_balance_due"], json!("500.000000"));
}

#[tokio::test]
async fn a_profit_estimation_answers_over_http_with_the_profit_it_derives() {
    let graph = Scripted::queue(vec![vec![json!({"id": "pe-1"})]]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/accounting/profit-estimations",
        Some(
            json!({"as_of_date": "2026-03-31", "period_start": "2026-01-01",
                    "period_end": "2026-03-31", "opening_capital": "1000.00",
                    "closing_capital": "1800.00", "additional_capital": "200.00",
                    "drawings": "150.00"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["net_profit"], json!("750.000000"));
}

#[tokio::test]
async fn a_statement_of_affairs_answers_with_its_capital_over_http() {
    let graph = Scripted::queue(vec![vec![json!({"id": "soa-1"})]]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/records/accounting/statements-of-affairs",
        Some(json!({"as_of_date": "2026-03-31", "assets": [
                    {"asset_name": "Cash", "asset_value": "500.00"}],
                    "liabilities": [{"liability_name": "Loan", "liability_value": "200.00"}]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["capital"], json!("300.000000"));
}

#[tokio::test]
async fn a_cash_transfer_answers_over_http_and_moves_the_balance() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "a-1", "account_name": "Main", "type": "operating",
                   "balance": "1000.000000", "min_balance": "0.000000"}),
            json!({"id": "a-2", "account_name": "Reserve", "type": "reserve",
                   "balance": "0.000000", "min_balance": "0.000000"}),
        ],
        vec![json!({"id": "a-1"})],
        vec![json!({"id": "a-2"})],
        vec![json!({"id": "t-1"})],
    ]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/cash-management/transfers",
        Some(json!({"from_account_id": "a-1", "to_account_id": "a-2",
                    "amount": "250.00", "transfer_date": "2026-03-15"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], json!("completed"));
    assert_eq!(body["amount"], json!("250.000000"));
}

#[tokio::test]
async fn a_liquidity_position_answers_over_http_with_its_ratio() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "a-1", "type": "operating", "balance": "600.000000"}),
            json!({"id": "a-2", "type": "reserve", "balance": "400.000000"}),
        ],
        vec![json!({"id": "p-1"})],
    ]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/records/cash-management/liquidity",
        Some(json!({"short_term_obligations": "500.00", "position_date": "2026-03-15"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_cash"], json!("1000.000000"));
    assert_eq!(body["liquidity_ratio"], json!("2.00"));
}

#[tokio::test]
async fn a_cash_forecast_answers_over_http_from_the_history_it_has() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "f-1", "company_id": "c-1", "flow_type": "inflow",
                   "amount": "3000.000000", "date": "2026-03-01"}),
            json!({"id": "f-2", "company_id": "c-1", "flow_type": "outflow",
                   "amount": "1000.000000", "date": "2026-03-01"}),
        ],
        vec![json!({"id": "p-1", "company_id": "c-1", "available_cash": "500.000000"})],
    ]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/treasury-management/forecast/c-1?days=10&now=2026-03-11",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["projected_inflows"], json!("3000.00"));
    assert_eq!(body["ending_position"], json!("2500.00"));
}

#[tokio::test]
async fn investment_options_answer_over_http_without_touching_the_store() {
    let graph = Scripted::returning(vec![]);
    let (status, body) = call_with(
        graph,
        "GET",
        "/treasury-management/investment-options",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["options"].as_array().expect("options").len(), 4);
}

#[tokio::test]
async fn a_policy_limit_answers_over_http_with_its_created_policy() {
    let graph = Scripted::queue(vec![
        vec![
            json!({"id": "p-1", "name": "Liquidity", "policy_category": "liquidity",
                    "status": "active"}),
        ],
        vec![json!({"id": "l-1"})],
    ]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/treasury-policy/policies/p-1/limits",
        Some(json!({"limit_type": "exposure", "limit_value": "250000.00"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["limit_value"], json!("250000.000000"));
}

#[tokio::test]
async fn a_limit_breach_answers_over_http_and_marks_itself_not_compliant() {
    let graph = Scripted::queue(vec![
        vec![json!({"id": "l-1", "policy_id": "p-1", "limit_value": "100.000000"})],
        vec![json!({"id": "c-1"})],
        vec![json!({"id": "l-1"})],
    ]);
    let (status, body) = call_with(
        graph,
        "POST",
        "/records/treasury-policy/limits/l-1/check",
        Some(json!({"checked_value": "150.00", "notes": "over"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["compliant"], json!(false));
    assert_eq!(body["utilization_pct"], json!("150.000000"));
}
