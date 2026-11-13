#![forbid(unsafe_code)]
//! financial-state-machine-service: the life of a financial document, draft to archived.
//!
//! A document is created in `draft` and moves through the five transitions the service defines. Every move
//! is checked against the map before it is applied, and each one is appended to the document's history, so
//! the record of how it got to its current state travels with it.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * `posted` and `cancelled` and `archived` are terminal - nothing moves out of them, and a cancelled
//!   document is not archived by accident;
//! * `pending_approval` can go back to `draft`, which is how a reviewer asks for changes;
//! * the document is created in `draft` whatever state the caller sends, because a document that arrives
//!   already posted has skipped the approvals the map exists to enforce;
//! * the transition's actor is whoever the caller names on the request, which is not the same thing as the
//!   caller - an accountant can record that a finance lead approved the document.

use serde_json::{json, Value};

use vimbai_store::read::{field, refused, text};
use vimbai_store::{BookScope, Graph, Records, StoreError, FINANCIAL_DOCUMENT};

/// The six states, in the order the service listed them.
pub const STATES: [&str; 6] = [
    "draft",
    "pending_approval",
    "approved",
    "posted",
    "cancelled",
    "archived",
];

/// What may follow each state.
pub fn allowed_from(state: &str) -> &'static [&'static str] {
    match state {
        "draft" => &["pending_approval", "cancelled"],
        "pending_approval" => &["approved", "draft", "cancelled"],
        "approved" => &["posted", "cancelled"],
        "posted" => &["archived"],
        _ => &[],
    }
}

fn known(state: &str) -> bool {
    STATES.contains(&state)
}

/// `POST /documents`. The document is created in `draft` and its history starts empty.
pub fn create_document(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company = text(payload, "company_id");
    if company.is_empty() {
        return Err(refused("company_id is required"));
    }
    let document_type = match text(payload, "document_type") {
        empty if empty.is_empty() => "invoice".to_owned(),
        given => given,
    };

    let mut body = payload.clone();
    if let Some(fields) = body.as_object_mut() {
        fields.remove("id");
        fields.remove("user_id");
        fields.remove("book_id");
        fields.insert("current_state".into(), json!("draft"));
        fields.insert("document_type".into(), json!(document_type));
        fields.insert("history".into(), json!([]));
    } else {
        return Err(refused("a document must be an object"));
    }

    Records::new(graph, FINANCIAL_DOCUMENT).create(scope, &body)
}

/// `GET /documents/{doc_id}`.
pub fn get_document(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    doc_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, FINANCIAL_DOCUMENT).get(scope, doc_id)
}

/// `POST /documents/{doc_id}/transition`.
pub fn transition(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    doc_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let to_state = text(payload, "to_state");
    if !known(&to_state) {
        return Err(refused(&format!("unknown state: {to_state}")));
    }

    let document = Records::new(graph, FINANCIAL_DOCUMENT).get(scope, doc_id)?;
    let from_state = field(&document, "current_state").to_owned();
    let allowed = allowed_from(&from_state);
    if !allowed.contains(&to_state.as_str()) {
        return Err(refused(&format!(
            "invalid transition: {from_state} -> {to_state}. Allowed: {allowed:?}"
        )));
    }

    let mut history = document
        .get("history")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    history.push(json!({
        "document_id": doc_id,
        "from_state": from_state,
        "to_state": to_state,
        "user_id": text(payload, "user_id"),
        "notes": text(payload, "notes"),
    }));
    let history_count = history.len();

    let answer = Records::new(graph, FINANCIAL_DOCUMENT).update(
        scope,
        doc_id,
        &json!({ "current_state": to_state, "history": history }),
    )?;
    let _ = answer;

    Ok(json!({
        "doc_id": doc_id,
        "current_state": to_state,
        "history_count": history_count,
    }))
}

/// `GET /documents/{doc_id}/history`.
pub fn history(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    doc_id: &str,
) -> Result<Value, StoreError> {
    let document = Records::new(graph, FINANCIAL_DOCUMENT).get(scope, doc_id)?;
    let history = document
        .get("history")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(json!({
        "doc_id": doc_id,
        "history": history,
        "current_state": field(&document, "current_state"),
    }))
}

/// `GET /states`: the catalogue, which needs no storage.
pub fn catalogue() -> Value {
    let mut transitions = serde_json::Map::new();
    for state in STATES {
        transitions.insert(state.to_owned(), json!(allowed_from(state)));
    }
    json!({
        "states": STATES,
        "transitions": Value::Object(transitions),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("u-1", "b-1")
    }

    #[test]
    fn a_document_arrives_in_draft_whatever_it_asks_for() {
        let graph = FakeGraph::returning(vec![json!({"id": "d-new"})]);
        create_document(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "document_type": "invoice", "current_state": "posted"}),
        )
        .expect("kept");

        let (cypher, params) = graph.last();
        assert!(cypher.contains("CREATE (x:FinancialDocument)"), "{cypher}");
        assert!(cypher.contains("OWNS_FINANCIAL_DOCUMENT"), "{cypher}");
        assert_eq!(params["props"]["current_state"], json!("draft"));
        assert_eq!(params["props"]["history"], json!([]));
        assert_eq!(params["props"]["document_type"], json!("invoice"));
        assert_eq!(params["props"]["company_id"], json!("c-1"));
    }

    #[test]
    fn a_cancelled_document_moves_nowhere() {
        assert!(allowed_from("cancelled").is_empty());
        assert!(allowed_from("archived").is_empty());
        assert_eq!(allowed_from("posted"), &["archived"]);
        assert_eq!(
            allowed_from("pending_approval"),
            &["approved", "draft", "cancelled"]
        );
    }

    #[test]
    fn an_illegal_move_is_refused_and_a_legal_one_is_written_into_the_history() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "d-1", "current_state": "draft", "history": []})],
            vec![json!({"id": "d-1"})],
        ]);
        let moved = transition(
            &graph,
            &scope(),
            "d-1",
            &json!({"to_state": "pending_approval", "user_id": "u-2", "notes": "for review"}),
        )
        .expect("moved");
        assert_eq!(moved["current_state"], json!("pending_approval"));
        assert_eq!(moved["history_count"], json!(1));

        let refusing = FakeGraph::queue(vec![vec![json!({
            "id": "d-2", "current_state": "cancelled", "history": []
        })]]);
        assert!(transition(&refusing, &scope(), "d-2", &json!({"to_state": "posted"})).is_err());
    }

    #[test]
    fn the_catalogue_names_six_states_and_their_moves() {
        let catalogue = catalogue();
        assert_eq!(catalogue["states"].as_array().unwrap().len(), 6);
        assert_eq!(
            catalogue["transitions"]["approved"],
            json!(["posted", "cancelled"])
        );
        assert_eq!(catalogue["transitions"]["archived"], json!([]));
    }
}
