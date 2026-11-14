//! Organisations over HTTP, and the gate that makes one the boundary of a Book.
//!
//! These walk the addresses a client calls rather than the crate functions: an organisation is opened, a Book is moved
//! into it, somebody is added, and then the same Book is read as each of three people - the owner, the member and a
//! stranger. The last one is what makes the boundary real: a stranger is refused by the engine with a 403, not handed an
//! empty listing that looks like an empty Book.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

const PREFIX: &str = "/identity";

/// One person, signed in: the token the identity surface reads, and the id the record routes read.
struct Person {
    id: String,
    token: String,
}

struct Client {
    graph: Arc<MemoryGraph>,
    person: Person,
}

impl Client {
    /// The record routes with the tenancy gate in front, which is how both binaries serve them.
    fn router(&self) -> axum::Router {
        let state: vimbai_api::records::GraphHandle = self.graph.clone();
        vimbai_api::tenancy_gate::guard(vimbai_api::records::router(state.clone()), state)
    }

    async fn send(
        &self,
        method: Method,
        uri: &str,
        book: Option<&str>,
        payload: Value,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", self.person.token))
            // what the gateway injects after verifying the token: the engine reads the caller from here
            .header("x-user-id", self.person.id.clone());
        if let Some(book) = book {
            builder = builder.header("x-book-id", book);
        }
        let response = self
            .router()
            .oneshot(builder.body(Body::from(payload.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn identity(&self, method: Method, path: &str, payload: Value) -> (StatusCode, Value) {
        self.send(method, &format!("{PREFIX}{path}"), None, payload)
            .await
    }

    async fn post_identity(&self, path: &str, payload: Value) -> (StatusCode, Value) {
        self.identity(Method::POST, path, payload).await
    }
}

async fn signed_in(graph: Arc<MemoryGraph>, username: &str, password: &str) -> Client {
    let bare = Client {
        graph: graph.clone(),
        person: Person {
            id: String::new(),
            token: String::new(),
        },
    };
    let (status, made) = bare
        .identity(
            Method::POST,
            "/users/register",
            json!({
                "email": format!("{username}@example.com"),
                "username": username,
                "first_name": "Tariro",
                "last_name": "Moyo",
                "role_ids": ["accountant"],
                "password": password,
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    let id = made["id"].as_str().expect("an id").to_string();
    let (status, session) = bare
        .identity(
            Method::POST,
            "/users/login",
            json!({ "username": username, "password": password }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{session}");
    Client {
        graph,
        person: Person {
            id,
            token: session["access_token"]
                .as_str()
                .expect("a token")
                .to_string(),
        },
    }
}

/// Open a Book as the caller, returning its id.
async fn open_book(client: &Client) -> String {
    let (status, made) = client
        .send(
            Method::POST,
            "/books",
            None,
            json!({"name": "Household", "tier": "household", "now": 1000.0,
                   "wrapped_book_key": "wrapped-for-user-1"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    made["book"]["id"].as_str().expect("an id").to_string()
}

#[tokio::test]
async fn an_organisation_is_opened_by_the_person_who_names_it_and_they_own_it() {
    let owner = signed_in(
        Arc::new(MemoryGraph::new()),
        "tariro",
        "correct horse battery",
    )
    .await;
    let (status, opened) = owner
        .post_identity("/organizations", json!({"name": "Mukandara Holdings"}))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{opened}");
    assert_eq!(opened["name"], json!("Mukandara Holdings"));
    assert_eq!(opened["slug"], json!("mukandara-holdings"));
    // the person who opens it is its owner, which is the membership every later question reads
    assert_eq!(opened["role"], json!("owner"));
    let organisation_id = opened["id"].as_str().expect("an id").to_string();

    let (status, listed) = owner
        .identity(Method::GET, "/organizations", json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["count"], json!(1));
    assert_eq!(listed["organizations"][0]["role"], json!("owner"));

    // somebody else has none of it, and is told the organisation is not there rather than that it is not theirs
    let stranger = signed_in(owner.graph.clone(), "sam", "another horse battery").await;
    let (status, empty) = stranger
        .identity(Method::GET, "/organizations", json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty["count"], json!(0));
    let (status, _) = stranger
        .identity(
            Method::GET,
            &format!("/organizations/{organisation_id}"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_member_of_the_organisation_may_open_its_book_and_a_stranger_may_not() {
    let owner = signed_in(
        Arc::new(MemoryGraph::new()),
        "tariro",
        "correct horse battery",
    )
    .await;
    let (_, opened) = owner
        .post_identity("/organizations", json!({"name": "Holdings"}))
        .await;
    let organisation_id = opened["id"].as_str().expect("an id").to_string();
    let book_id = open_book(&owner).await;

    // the Book moves into the organisation: the caller has to administer the organisation and hold the Book
    let (status, moved) = owner
        .identity(
            Method::PUT,
            &format!("/organizations/{organisation_id}/books/{book_id}"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{moved}");
    assert_eq!(moved["organisation_id"], json!(organisation_id));

    let (status, books) = owner
        .identity(
            Method::GET,
            &format!("/organizations/{organisation_id}/books"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{books}");
    assert_eq!(books["count"], json!(1));
    assert_eq!(books["books"][0]["id"], json!(book_id));

    // somebody added to the organisation, with no membership row of the Book at all
    let member = signed_in(owner.graph.clone(), "sam", "another horse battery").await;
    let (status, added) = owner
        .post_identity(
            &format!("/organizations/{organisation_id}/members"),
            json!({"user_id": member.person.id, "role": "member"}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{added}");
    assert_eq!(added["role"], json!("member"));

    // ... reads the Book
    let (status, invoices) = member
        .send(Method::GET, "/invoices", Some(&book_id), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{invoices}");

    // ... and the gateway's membership check answers for the organisation
    let (status, answer) = member
        .send(
            Method::GET,
            &format!("/books/{book_id}/membership"),
            Some(&book_id),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["via"], json!("organisation"));
    assert_eq!(answer["role"], json!("member"));

    // somebody in nobody's organisation is refused by the engine itself, not handed an empty Book
    let stranger = signed_in(owner.graph.clone(), "rutendo", "a third horse battery").await;
    let (status, refusal) = stranger
        .send(Method::GET, "/invoices", Some(&book_id), json!({}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refusal}");
    assert_eq!(refusal["book_id"], json!(book_id));
}

#[tokio::test]
async fn moving_a_book_needs_both_halves_so_a_member_cannot_take_one() {
    let owner = signed_in(
        Arc::new(MemoryGraph::new()),
        "tariro",
        "correct horse battery",
    )
    .await;
    let (_, opened) = owner
        .post_identity("/organizations", json!({"name": "One"}))
        .await;
    let organisation_id = opened["id"].as_str().expect("an id").to_string();
    let book_id = open_book(&owner).await;

    // user-2 is a member of the organisation: they may read its Books, and may not change them
    let member = signed_in(owner.graph.clone(), "sam", "another horse battery").await;
    let (status, _) = owner
        .post_identity(
            &format!("/organizations/{organisation_id}/members"),
            json!({"user_id": member.person.id, "role": "member"}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, refusal) = member
        .identity(
            Method::PUT,
            &format!("/organizations/{organisation_id}/books/{book_id}"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refusal}");
}

#[tokio::test]
async fn an_organisation_keeps_at_least_one_owner() {
    let owner = signed_in(
        Arc::new(MemoryGraph::new()),
        "tariro",
        "correct horse battery",
    )
    .await;
    let (_, opened) = owner
        .post_identity("/organizations", json!({"name": "One"}))
        .await;
    let organisation_id = opened["id"].as_str().expect("an id").to_string();

    // the only owner cannot demote themselves out of owning it
    let (status, refusal) = owner
        .post_identity(
            &format!("/organizations/{organisation_id}/members"),
            json!({"user_id": owner.person.id, "role": "member"}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{refusal}");

    // a role the organisation does not know is refused by name rather than stored and ignored
    let (status, refusal) = owner
        .post_identity(
            &format!("/organizations/{organisation_id}/members"),
            json!({"user_id": "user-2", "role": "superuser"}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refusal}");
}

#[tokio::test]
async fn the_members_of_an_organisation_are_listed_with_their_roles() {
    let owner = signed_in(
        Arc::new(MemoryGraph::new()),
        "tariro",
        "correct horse battery",
    )
    .await;
    let (_, opened) = owner
        .post_identity("/organizations", json!({"name": "One"}))
        .await;
    let organisation_id = opened["id"].as_str().expect("an id").to_string();
    let member = signed_in(owner.graph.clone(), "sam", "another horse battery").await;
    let (status, _) = owner
        .post_identity(
            &format!("/organizations/{organisation_id}/members"),
            json!({"user_id": member.person.id}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, listed) = owner
        .identity(
            Method::GET,
            &format!("/organizations/{organisation_id}/members"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["count"], json!(2));
    let roles: Vec<String> = listed["members"]
        .as_array()
        .expect("members")
        .iter()
        .map(|row| row["role"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(roles.contains(&"owner".to_string()), "{roles:?}");
    // a member added with no role stated is a member, not an owner
    assert!(roles.contains(&"member".to_string()), "{roles:?}");
}
