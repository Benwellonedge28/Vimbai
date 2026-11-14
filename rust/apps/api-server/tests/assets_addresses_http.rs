//! biological-assets-service and investment-property-service: the calculators behind the addresses they published.
//!
//! Both services were pure calculators: they read a request, computed, and answered. Neither owned a record, which is
//! why their addresses can be served from the engine directly. These tests walk each published address over HTTP, at the
//! path the gateway forwards, and check the answers against the arithmetic the Python service performed.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

struct Engine {
    router: axum::Router,
}

impl Engine {
    fn new() -> Self {
        // these addresses are calculators, not records, so the whole engine is the right surface
        Self {
            router: vimbai_api::router(),
        }
    }

    async fn post(&self, uri: &str, payload: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-user-id", "user-1")
            .header("x-book-id", "book-1")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

/// An amount read as cents, so a figure is compared by value rather than by how many zeros it carries.
fn cents(raw: &Value) -> i128 {
    text_cents(raw.as_str().unwrap_or("0"))
}

fn text_cents(text: &str) -> i128 {
    let (whole, part) = text.split_once('.').unwrap_or((text, ""));
    let negative = whole.starts_with('-');
    let whole: i128 = whole.trim_start_matches('-').parse().unwrap_or(0);
    let pad = format!("{:0<2}", &part[..part.len().min(2)]);
    let total = whole * 100 + pad.parse::<i128>().unwrap_or(0);
    if negative {
        -total
    } else {
        total
    }
}

// ---------------------------------------------------------------- biological assets

#[tokio::test]
async fn a_quoted_market_price_measures_an_asset_at_level_one() {
    let engine = Engine::new();
    let (status, answer) = engine
        .post(
            "/biological-assets/fair-value-level1",
            json!({
                "asset_quantity": "200",
                "quoted_price_per_unit": "15.50",
                "costs_to_sell_percentage": 0.05,
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {answer}");
    // 200 head at 15.50 is 3100 gross, 5% to sell is 155, so 2945 at fair value
    assert_eq!(cents(&answer["fair_value"]), 294_500, "{answer}");
    assert_eq!(cents(&answer["costs_to_sell"]), 15_500, "{answer}");
    // 15.50 less 5% is 14.725, which rounds half to even at two places
    assert_eq!(cents(&answer["fair_value_per_unit"]), 1_472, "{answer}");
    assert_eq!(answer["level"], 1, "{answer}");
}

#[tokio::test]
async fn a_similar_asset_in_the_market_measures_at_level_two() {
    let engine = Engine::new();
    let (status, answer) = engine
        .post(
            "/biological-assets/fair-value-level2",
            json!({
                "asset_quantity": "100",
                "market_price_similar": "20",
                "adjustment_factor": 0.9,
                "costs_to_sell_percentage": 0.10,
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {answer}");
    // 20 adjusted down by a tenth is 18, times 100 is 1800, a tenth to sell is 180, so 1620
    assert_eq!(cents(&answer["adjusted_price"]), 1_800, "{answer}");
    assert_eq!(cents(&answer["costs_to_sell"]), 18_000, "{answer}");
    assert_eq!(cents(&answer["fair_value"]), 162_000, "{answer}");
    assert_eq!(answer["level"], 2, "{answer}");
}

#[tokio::test]
async fn a_discounted_cash_flow_measures_at_level_three() {
    let engine = Engine::new();
    let (status, answer) = engine
        .post(
            "/biological-assets/fair-value-level3",
            json!({
                "present_value_fcfe": "100000",
                "terminal_value": "20000",
                "costs_to_sell": "3000",
                "discount_rate": 0.12,
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {answer}");
    assert_eq!(answer["level"], 3, "{answer}");
    // present value plus terminal value less what it costs to sell
    assert_eq!(cents(&answer["fair_value"]), 11_700_000, "{answer}");
}

#[tokio::test]
async fn a_bearer_animal_is_told_apart_from_a_consumable_one() {
    let engine = Engine::new();
    let (status, bearer) = engine
        .post(
            "/biological-assets/consumable-bearer-distinction",
            json!({
                "purpose": "dairy",
                "reproduction_purpose": "breeding",
                "harvesting_intent": "none",
                "historical_use": "producing milk for several seasons",
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {bearer}");
    let classification = bearer["classification"]
        .as_str()
        .or_else(|| bearer["asset_type"].as_str())
        .unwrap_or_default();
    assert!(
        classification.to_lowercase().contains("bearer"),
        "a dairy herd is a bearer asset: {bearer}"
    );
}

#[tokio::test]
async fn the_disclosure_counts_the_assets_the_caller_brought() {
    let engine = Engine::new();
    let (status, disclosure) = engine
        .post(
            "/biological-assets/disclosure-template",
            json!({
                "reporting_date": "2026-10-31",
                "assets": [
                    {
                        "asset_id": "a1", "asset_name": "Dairy cows", "asset_category": "bearer_biological",
                        "species": "cattle", "quantity": "120", "unit": "head", "age_years": 4,
                        "maturity_status": "mature", "location": "Chinhoyi"
                    },
                    {
                        "asset_id": "a2", "asset_name": "Maize crop", "asset_category": "consumable_biological",
                        "species": "maize", "quantity": "40", "unit": "hectares", "age_years": 0.5,
                        "maturity_status": "immature", "location": "Chinhoyi"
                    }
                ],
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {disclosure}");
    assert_eq!(disclosure["reporting_date"], "2026-10-31", "{disclosure}");
    assert_eq!(disclosure["assets_by_category"]["bearer_biological"], 1);
    assert_eq!(disclosure["assets_by_category"]["consumable_biological"], 1);
    assert_eq!(disclosure["assets_by_location"]["Chinhoyi"], 2);
    assert!(
        disclosure["disclosure_sections"]
            .as_array()
            .map(|sections| sections.len() >= 5)
            .unwrap_or(false),
        "{disclosure}"
    );
}

#[tokio::test]
async fn measuring_a_herd_answers_at_the_published_address() {
    let engine = Engine::new();
    let (status, measured) = engine
        .post(
            "/biological-assets/measure",
            json!({
                "company_id": "company-1",
                "reporting_date": "2026-10-31",
                "assets": [
                    {
                        "asset_id": "a1", "asset_name": "Dairy cows", "asset_category": "bearer_biological",
                        "species": "cattle", "quantity": "120", "unit": "head", "age_years": 4,
                        "maturity_status": "mature", "location": "Chinhoyi", "initial_cost": "18000",
                        "acquisition_date": "2024-06-01"
                    }
                ],
                "transformations": [],
                "harvests": [],
                "include_disclosure": true,
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {measured}");
    // 120 head at an estimated 4750 each after costs of selling, total 570000
    assert_eq!(
        cents(&measured["total_biological_assets"]),
        57_000_000,
        "{measured}"
    );
    assert_eq!(measured["maturity_analysis"]["mature"], 1, "{measured}");
    assert_eq!(
        cents(&measured["fair_value_measurements"][0]["fair_value_per_unit"]),
        475_000,
        "{measured}"
    );
}

// ---------------------------------------------------------------- investment property

fn a_property() -> Value {
    json!({
        "property_id": "prop-1",
        "property_name": "Borrowdale office block",
        "property_type": "office",
        "location": "Harare",
        "acquisition_date": "2021-03-01",
        "acquisition_cost": "400000",
        "fair_value_last_reporting": "500000",
        "useful_life_years": 40,
        "rental_income_annual": "48000",
        "operating_expenses_annual": "14000",
    })
}

#[tokio::test]
async fn the_fair_value_model_revalues_a_property() {
    let engine = Engine::new();
    let (status, answer) = engine
        .post(
            "/investment-property/fair-value-model",
            json!({
                "company_id": "company-1",
                "reporting_date": "2026-10-31",
                "properties": [a_property()],
                "current_fair_values": {"prop-1": "540000"},
                "rental_income": "48000",
                "operating_expenses": "14000",
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {answer}");
    // carried at 500000, revalued to 540000, so 40000 of gain
    assert_eq!(
        cents(&answer["fair_value_measurements"][0]["fair_value_gain_loss"]),
        4_000_000,
        "{answer}"
    );
    // a revaluation gain on investment property goes to other comprehensive income
    assert_eq!(
        cents(&answer["fair_value_gain_loss_in_oci"]),
        4_000_000,
        "{answer}"
    );
    assert_eq!(
        cents(&answer["total_fair_value_or_carrying_amount"]),
        54_000_000,
        "{answer}"
    );
}

#[tokio::test]
async fn the_cost_model_charges_depreciation_instead_of_revaluing() {
    let engine = Engine::new();
    let (status, answer) = engine
        .post(
            "/investment-property/cost-model",
            json!({
                "company_id": "company-1",
                "reporting_date": "2026-10-31",
                "properties": [a_property()],
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {answer}");
    let carried = &answer["cost_model_depreciations"][0];
    assert_eq!(cents(&carried["cost"]), 40_000_000, "{answer}");
    assert!(
        cents(&carried["carrying_amount"]) < 40_000_000,
        "a depreciated property cannot be carried above its cost: {answer}"
    );
    assert_eq!(
        cents(&answer["direct_expenses_including_depreciation"]),
        1_000_000,
        "the depreciation charge belongs in the answer: {answer}"
    );
}

#[tokio::test]
async fn a_transfer_moves_property_between_models() {
    let engine = Engine::new();
    let (status, answer) = engine
        .post(
            "/investment-property/transfer",
            json!({
                "property_id": "prop-1",
                "reason_for_transfer": "now held to earn rentals",
                "model_from": "owner_occupied",
                "model_to": "investment",
                "carrying_amount": "380000",
                "fair_value": "520000",
                "transfer_date": "2026-10-31",
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {answer}");
    // 380000 carried, revalued to 520000, so 140000 of revaluation surplus
    // 380000 carried, transferred at 520000, so 140000 of adjustment taken to OCI
    assert_eq!(
        cents(&answer["adjustment_to_property"]),
        14_000_000,
        "{answer}"
    );
    assert_eq!(answer["treatment"], "recognized in OCI", "{answer}");
}

#[tokio::test]
async fn the_disclosure_lists_the_properties_at_their_fair_values() {
    let engine = Engine::new();
    let (status, answer) = engine
        .post(
            "/investment-property/fair-value-disclosure",
            json!({
                "reporting_date": "2026-10-31",
                "properties": [a_property()],
                "current_fair_values": {"prop-1": "540000"},
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {answer}");
    assert_eq!(answer["total_properties"], 1, "{answer}");
    assert_eq!(cents(&answer["level_1_fair_value"]), 54_000_000, "{answer}");
}

#[tokio::test]
async fn the_yields_are_worked_out_from_rent_and_value() {
    let engine = Engine::new();
    let (status, answer) = engine
        .post(
            "/investment-property/yields",
            json!({
                "rental_income_annual": "48000",
                "current_fair_value": "600000",
                "capitalization_rate": 8,
            }),
        )
        .await;
    assert!(status.is_success(), "{status}: {answer}");
    // 48000 on 600000 is an 8% gross yield, and after 30% expenses a 5.6% net yield
    assert_eq!(answer["gross_yield"].as_f64().unwrap() as i64, 8);
    assert_eq!(answer["net_yield"].as_f64().unwrap() as i64, 5);
    // a 48000 rent capitalised at 8% implies 600000 of value
    assert_eq!(
        cents(&answer["market_value_implied"]),
        60_000_000,
        "{answer}"
    );
}
