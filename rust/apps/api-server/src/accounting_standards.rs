//! accounting-standards-service on HTTP.
//!
//! The catalogue endpoints need no store; the configuration, mapping, policy and compliance routes
//! do, and are Book-scoped like every other record.

use accounting::standards as service;
use axum::{
    extract::{Path, RawQuery, State},
    http::HeaderMap,
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use store::StoreError;

use crate::records::{
    answer, body_or_query, overlay, query_object, query_param, scope_of, GraphHandle,
};

type GraphState = State<GraphHandle>;

fn catalogue(value: Value) -> Response {
    answer(Ok::<Value, StoreError>(value))
}

async fn list_standards(headers: HeaderMap, RawQuery(query): RawQuery) -> Response {
    // the catalogue is code-defined and needs no store, but the caller's scope is still checked
    match scope_of(&headers) {
        Ok(_) => (),
        Err(response) => return *response,
    }
    catalogue(service::list_standards(&query_object(query.as_deref())))
}

async fn get_standard(headers: HeaderMap, Path(standard): Path<String>) -> Response {
    match scope_of(&headers) {
        Ok(_) => (),
        Err(response) => return *response,
    }
    answer(service::get_standard(&standard))
}

async fn compare_standards(headers: HeaderMap, RawQuery(query): RawQuery) -> Response {
    match scope_of(&headers) {
        Ok(_) => (),
        Err(response) => return *response,
    }
    let first = query_param(query.as_deref(), "standard_1").unwrap_or_default();
    let second = query_param(query.as_deref(), "standard_2").unwrap_or_default();
    answer(service::compare_standards(&first, &second))
}

async fn standard_categories(headers: HeaderMap) -> Response {
    match scope_of(&headers) {
        Ok(_) => (),
        Err(response) => return *response,
    }
    catalogue(service::standard_categories())
}

async fn standard_requirements(headers: HeaderMap, Path(standard): Path<String>) -> Response {
    match scope_of(&headers) {
        Ok(_) => (),
        Err(response) => return *response,
    }
    catalogue(service::standard_requirements(&standard))
}

async fn disclosure_requirements(headers: HeaderMap, Path(standard): Path<String>) -> Response {
    match scope_of(&headers) {
        Ok(_) => (),
        Err(response) => return *response,
    }
    catalogue(service::disclosure_requirements(&standard))
}

async fn measurement_guide(headers: HeaderMap, Path(standard): Path<String>) -> Response {
    match scope_of(&headers) {
        Ok(_) => (),
        Err(response) => return *response,
    }
    answer(service::measurement_guide(&standard))
}

async fn configure_organization(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(organization_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(service::configure_organization(
        graph.as_ref(),
        &scope,
        &organization_id,
        &body,
    ))
}

async fn get_configuration(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(organization_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(service::get_configuration(
        graph.as_ref(),
        &scope,
        &organization_id,
    ))
}

async fn update_configuration(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(organization_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(service::update_configuration(
        graph.as_ref(),
        &scope,
        &organization_id,
        &body,
    ))
}

async fn activate_standard(
    State(graph): GraphState,
    headers: HeaderMap,
    Path((organization_id, standard)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(service::activate_standard(
        graph.as_ref(),
        &scope,
        &organization_id,
        &standard,
        &body,
    ))
}

async fn deactivate_standard(
    State(graph): GraphState,
    headers: HeaderMap,
    Path((organization_id, standard)): Path<(String, String)>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(service::deactivate_standard(
        graph.as_ref(),
        &scope,
        &organization_id,
        &standard,
        &body,
    ))
}

async fn account_mappings(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(standard): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(service::account_mappings(graph.as_ref(), &scope, &standard))
}

async fn add_account_mapping(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(standard): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(service::add_account_mapping(
        graph.as_ref(),
        &scope,
        &standard,
        &body,
    ))
}

async fn create_policy(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(organization_id): Path<String>,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = payload
        .map(|Json(value)| value)
        .unwrap_or_else(|| json!({}));
    answer(service::create_policy(
        graph.as_ref(),
        &scope,
        &organization_id,
        &body,
    ))
}

async fn list_policies(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(organization_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(service::list_policies(
        graph.as_ref(),
        &scope,
        &organization_id,
        &filters,
    ))
}

async fn policy_for_area(
    State(graph): GraphState,
    headers: HeaderMap,
    Path((organization_id, policy_area)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let filters = query_object(query.as_deref());
    answer(service::policy_for_area(
        graph.as_ref(),
        &scope,
        &organization_id,
        &policy_area,
        &filters,
    ))
}

async fn run_compliance_check(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(organization_id): Path<String>,
    RawQuery(query): RawQuery,
    payload: Option<Json<Value>>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let body = overlay(
        body_or_query(query.as_deref()),
        payload.map(|Json(value)| value),
    );
    answer(service::run_compliance_check(
        graph.as_ref(),
        &scope,
        &organization_id,
        &body,
    ))
}

async fn compliance_history(
    State(graph): GraphState,
    headers: HeaderMap,
    Path(organization_id): Path<String>,
) -> Response {
    let scope = match scope_of(&headers) {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    answer(service::compliance_history(
        graph.as_ref(),
        &scope,
        &organization_id,
    ))
}

/// The service's own paths, and the same routes under the record prefix.
pub fn router(graph: GraphHandle) -> Router {
    let mut router = Router::new();
    for prefix in ["/accounting-standards", "/records/accounting-standards"] {
        router = router
            .route(&format!("{prefix}/standards"), get(list_standards))
            .route(
                &format!("{prefix}/standards/comparison"),
                get(compare_standards),
            )
            .route(
                &format!("{prefix}/standards/categories"),
                get(standard_categories),
            )
            .route(
                &format!("{prefix}/standards/:standard_type"),
                get(get_standard),
            )
            .route(
                &format!("{prefix}/standards/:standard_type/requirements"),
                get(standard_requirements),
            )
            .route(
                &format!("{prefix}/standards/:standard_type/disclosure-requirements"),
                get(disclosure_requirements),
            )
            .route(
                &format!("{prefix}/standards/:standard_type/measurement-guide"),
                get(measurement_guide),
            )
            .route(
                &format!("{prefix}/standards/:standard_type/account-mapping"),
                get(account_mappings).post(add_account_mapping),
            )
            .route(
                &format!("{prefix}/organizations/:organization_id/configuration"),
                post(configure_organization)
                    .get(get_configuration)
                    .put(update_configuration),
            )
            .route(
                &format!(
                    "{prefix}/organizations/:organization_id/standards/:standard_type/activate"
                ),
                post(activate_standard),
            )
            .route(
                &format!(
                    "{prefix}/organizations/:organization_id/standards/:standard_type/deactivate"
                ),
                post(deactivate_standard),
            )
            .route(
                &format!("{prefix}/organizations/:organization_id/policies"),
                post(create_policy).get(list_policies),
            )
            .route(
                &format!("{prefix}/organizations/:organization_id/policies/:policy_area"),
                get(policy_for_area),
            )
            .route(
                &format!("{prefix}/organizations/:organization_id/compliance/check"),
                post(run_compliance_check),
            )
            .route(
                &format!("{prefix}/organizations/:organization_id/compliance/history"),
                get(compliance_history),
            );
    }
    router.with_state(graph)
}
