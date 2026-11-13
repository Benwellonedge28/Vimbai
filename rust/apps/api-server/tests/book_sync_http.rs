//! book-sync-service over HTTP: opening a shared Book, inviting someone, pushing and pulling encrypted entries.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

struct Client {
    router: axum::Router,
    graph: Arc<MemoryGraph>,
    user: &'static str,
}

impl Client {
    fn new() -> Self {
        let graph = Arc::new(MemoryGraph::new());
        Self {
            router: vimbai_api::records::router(graph.clone()),
            graph,
            user: "user-1",
        }
    }

    /// The same store, seen as another person: a Book's members are different people reading one graph.
    fn as_user(&self, user: &'static str) -> Self {
        Self {
            router: vimbai_api::records::router(self.graph.clone()),
            graph: self.graph.clone(),
            user,
        }
    }

    async fn call(&self, method: Method, uri: &str, payload: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-user-id", self.user)
            .header("x-book-id", "caller-book")
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

#[tokio::test]
async fn a_shared_book_is_opened_joined_written_to_and_pulled() {
    let client = Client::new();
    let (status, made) = client
        .call(
            Method::POST,
            "/books",
            json!({"name": "Household", "tier": "household", "now": 1000.0,
                   "wrapped_book_key": "wrapped-for-user-1"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["your_role"], json!("owner"));
    let book_id = made["book"]["id"].as_str().expect("an id").to_string();

    // a tier the Book does not know is refused by name
    let (status, _) = client
        .call(
            Method::POST,
            "/books",
            json!({"name": "Odd", "tier": "charity", "now": 1000.0}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // an invitation must carry the Book key wrapped for the person invited
    let (status, _) = client
        .call(
            Method::POST,
            &format!("/books/{book_id}/members"),
            json!({"user_id": "user-2", "role": "viewer"}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, invited) = client
        .call(
            Method::POST,
            &format!("/books/{book_id}/members"),
            json!({"user_id": "user-2", "role": "bookkeeper", "wrapped_book_key": "wrapped-for-user-2",
                   "now": 1001.0}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{invited}");

    let (_, own) = client.call(Method::GET, "/books", json!({})).await;
    assert_eq!(
        own["books"].as_array().map(Vec::len),
        Some(1),
        "owner listing {own}"
    );
    let (_, people) = client
        .call(Method::GET, &format!("/books/{book_id}/members"), json!({}))
        .await;
    assert_eq!(
        people["members"].as_array().map(Vec::len),
        Some(2),
        "members {people}"
    );

    // the invited person sees the Book as pending and cannot read it yet
    let guest = client.as_user("user-2");
    let (status, listed) = guest.call(Method::GET, "/books", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(
        listed["books"][0]["membership_status"],
        json!("invited"),
        "listing was {listed}"
    );
    let (status, _) = guest
        .call(Method::GET, &format!("/books/{book_id}"), json!({}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, accepted) = guest
        .call(
            Method::POST,
            &format!("/books/{book_id}/members/accept"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["wrapped_book_key"], json!("wrapped-for-user-2"));
    let (status, check) = guest
        .call(
            Method::GET,
            &format!("/books/{book_id}/membership"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{check}");
    assert_eq!(check["role"], json!("bookkeeper"));

    // the new member registers a device and pushes an encrypted entry
    let (status, device) = guest
        .call(
            Method::POST,
            &format!("/books/{book_id}/devices"),
            json!({"device_name": "phone", "now": 1002.0}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{device}");
    let device_id = device["device_id"].as_str().expect("a device").to_string();
    let batch = json!({"device_id": device_id, "entries": [
        {"entry_id": "e1", "payload": "cipher-1", "created_at": 1003.0}
    ]});
    let (status, pushed) = guest
        .call(
            Method::POST,
            &format!("/books/{book_id}/entries/push"),
            batch.clone(),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{pushed}");
    assert_eq!(pushed["accepted"], json!(1));
    assert_eq!(pushed["book_seq"], json!(1));
    // the same batch again stores nothing, which is what makes a retry safe
    let (_, repeated) = guest
        .call(
            Method::POST,
            &format!("/books/{book_id}/entries/push"),
            batch,
        )
        .await;
    assert_eq!(repeated["accepted"], json!(0));

    // the owner pulls it, and the query-string sequence is honoured rather than ignored
    let (status, pulled) = client
        .call(Method::GET, &format!("/books/{book_id}/entries"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{pulled}");
    assert_eq!(pulled["entries"][0]["payload"], json!("cipher-1"));
    assert_eq!(pulled["entries"][0]["user_id"], json!("user-2"));
    let (_, later) = client
        .call(
            Method::GET,
            &format!("/books/{book_id}/entries?since=1"),
            json!({}),
        )
        .await;
    assert_eq!(later["entries"].as_array().map(Vec::len), Some(0));

    // metadata is last write wins, and the trail says who invited whom
    let (status, stored) = client
        .call(
            Method::POST,
            &format!("/books/{book_id}/metadata"),
            json!({"key": "settings", "value": "one", "updated_at": 1004.0}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{stored}");
    let (_, meta) = client
        .call(
            Method::GET,
            &format!("/books/{book_id}/metadata?since=1000"),
            json!({}),
        )
        .await;
    let keys: Vec<&Value> = meta["metadata"]
        .as_array()
        .expect("metadata")
        .iter()
        .map(|row| &row["key"])
        .collect();
    assert!(keys.contains(&&json!("settings")));
    let (_, trail) = client
        .call(Method::GET, &format!("/books/{book_id}/audit"), json!({}))
        .await;
    let actions: Vec<&Value> = trail["audit"]
        .as_array()
        .expect("audit")
        .iter()
        .map(|row| &row["action"])
        .collect();
    assert!(actions.contains(&&json!("member.invited")));

    // a stranger reads nothing, and a personal Book is provisioned once
    let stranger = client.as_user("user-9");
    let (status, _) = stranger
        .call(Method::GET, &format!("/books/{book_id}"), json!({}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, first) = stranger
        .call(Method::POST, "/books/default", json!({"now": 1005.0}))
        .await;
    assert_eq!(first["created"], json!(true));
    let (_, again) = stranger
        .call(Method::POST, "/books/default", json!({"now": 1006.0}))
        .await;
    assert_eq!(again["created"], json!(false));
}
