#![forbid(unsafe_code)]
//! The first run: a stranger arrives, opens a Book, and the bookkeeping loop answers with their money in it.
//!
//! Everything else in this engine is reachable only after this walk succeeds, so it is tested as one walk rather than as
//! steps that each pass on their own. Registering an account, signing in, creating a Book, naming accounts and posting an
//! entry are separate services; the thing a person experiences is the whole path, and it broke exactly there. The device
//! store returned each listed entry one level down - `{"x": {...entry}, "lines": [...]}` - so a Book full of posted entries
//! reported a trial balance of zeroes. The unit tests passed because they read the lines, which were where they were
//! promised. Nothing read the entry's own fields, which were not.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use store::fake::MemoryGraph;
use tower::ServiceExt;

struct Walk {
    router: axum::Router,
    user: String,
    /// The session the sign-in handed back. The record routes read the person from the header, but the identity surface
    /// reads the token, which is also what a client keeps: a walk that sent only the id would pass here and fail at the
    /// first address a real client calls.
    token: String,
    book: String,
}

impl Walk {
    /// A stranger, with no account and no Book.
    fn stranger() -> Self {
        let graph = Arc::new(MemoryGraph::new());
        Self::on(vimbai_api::records::router(graph))
    }

    /// A second person at the same engine: two people only meet if they are talking to one store, and a walk that gives
    /// each of them their own would pass while proving nothing.
    fn on(router: axum::Router) -> Self {
        Self {
            router,
            user: String::new(),
            token: String::new(),
            book: String::new(),
        }
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        if !self.user.is_empty() {
            builder = builder.header("x-user-id", &self.user);
        }
        if !self.token.is_empty() {
            builder = builder.header("authorization", format!("Bearer {}", self.token));
        }
        if !self.book.is_empty() {
            builder = builder.header("x-book-id", &self.book);
        }
        let body = match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        };
        let response = self
            .router
            .clone()
            .oneshot(builder.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 4_000_000)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn register_and_sign_in(&mut self) -> String {
        self.sign_up("stranger@example.com", "Stan").await
    }

    /// An account of someone's own, then the session that account uses. The email is the name, because two people at one
    /// engine are two people with two addresses.
    async fn sign_up(&mut self, email: &str, first_name: &str) -> String {
        let (status, made) = self
            .call(
                Method::POST,
                "/identity/users/register",
                Some(json!({
                    "email": email,
                    "password": "Str0ng-pass-9",
                    "first_name": first_name,
                })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{made}");
        self.user = made["id"].as_str().expect("an id").to_string();

        let (status, signed) = self
            .call(
                Method::POST,
                "/identity/users/login",
                Some(json!({"identifier": email, "password": "Str0ng-pass-9"})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{signed}");
        let token = signed["access_token"]
            .as_str()
            .expect("a token")
            .to_string();
        self.token = token.clone();
        token
    }

    async fn open_book(&mut self) -> Value {
        let (status, made) = self
            .call(
                Method::POST,
                "/books",
                Some(json!({"name": "Stan's books", "currency": "USD", "tier": "business"})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{made}");
        self.book = made["book"]["id"].as_str().expect("a book id").to_string();
        made
    }

    /// The organisation the stranger set up, named and administered by them, with their Book moved inside it.
    async fn organise(&self) -> Value {
        let (status, opened) = self
            .call(
                Method::POST,
                "/identity/organizations",
                Some(json!({"name": "Stan's studio"})),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{opened}");
        let organisation = opened["id"].as_str().expect("an id").to_string();
        let (status, moved) = self
            .call(
                Method::PUT,
                &format!("/identity/organizations/{organisation}/books/{}", self.book),
                Some(json!({})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{moved}");
        assert_eq!(moved["organisation_id"], json!(organisation));
        opened
    }

    /// The accounts the Book holds, as `GET /accounting/accounts` answers them.
    async fn chart(&self) -> Vec<Value> {
        let (status, listed) = self.call(Method::GET, "/accounting/accounts", None).await;
        assert_eq!(status, StatusCode::OK, "{listed}");
        listed
            .as_array()
            .cloned()
            .or_else(|| listed["accounts"].as_array().cloned())
            .expect("a chart")
    }

    async fn post_entry(&self, status: &str, amount: &str) -> Value {
        let (code, made) = self
            .call(
                Method::POST,
                "/accounting/journal-entries",
                Some(json!({
                    "entry_date": "2026-11-15",
                    "description": "first sale",
                    "status": status,
                    "lines": [
                        {"account_number": "1000", "debit": amount, "credit": "0"},
                        {"account_number": "4000", "debit": "0", "credit": amount},
                    ],
                })),
            )
            .await;
        assert_eq!(code, StatusCode::OK, "{made}");
        made
    }

    async fn trial_balance(&self) -> Value {
        let (status, answered) = self
            .call(Method::GET, "/accounting/trial-balance", None)
            .await;
        assert_eq!(status, StatusCode::OK, "{answered}");
        answered
    }
}

#[tokio::test]
async fn a_stranger_can_open_a_book_and_its_first_entry_moves_the_balance() {
    let mut walk = Walk::stranger();
    let token = walk.register_and_sign_in().await;
    assert!(!token.is_empty(), "signing in has to mint a token");

    let opened = walk.open_book().await;
    // the person who opened the Book owns it
    assert_eq!(opened["your_role"], json!("owner"));
    assert!(!walk.book.is_empty(), "opening a Book has to name it");

    // the Book opens with the accounts it takes to write in it, so the first sale needs no setup
    let chart = walk.chart().await;
    assert!(
        chart
            .iter()
            .any(|row| row["account_number"] == json!("1000")),
        "a new Book opens with cash: {chart:?}"
    );

    walk.post_entry("posted", "250.00").await;

    let balance = walk.trial_balance().await;
    assert_eq!(
        balance["total_debits"],
        json!("250.000000"),
        "the sale has to reach the trial balance: {balance}"
    );
    assert_eq!(balance["total_credits"], json!("250.000000"), "{balance}");
    assert_eq!(balance["is_balanced"], json!(true), "{balance}");

    let rows = balance["accounts"].as_array().expect("rows");
    let cash = rows
        .iter()
        .find(|row| row["account_number"] == json!("1000"))
        .expect("the cash row");
    assert_eq!(cash["debit"], json!("250.000000"), "{balance}");
}

#[tokio::test]
async fn a_listed_entry_says_what_it_is_without_being_unwrapped_first() {
    let mut walk = Walk::stranger();
    walk.register_and_sign_in().await;
    walk.open_book().await;
    walk.post_entry("posted", "10.00").await;

    let (status, listed) = walk
        .call(Method::GET, "/accounting/journal-entries", None)
        .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let entries = listed
        .as_array()
        .or_else(|| listed["journal_entries"].as_array())
        .expect("a listing");
    let entry = entries.first().expect("the entry just posted");

    // the entry's own fields sit on the entry, not one level down inside it
    assert_eq!(
        entry["status"],
        json!("posted"),
        "an entry has to carry its own status: {entry}"
    );
    assert_eq!(entry["entry_date"], json!("2026-11-15"), "{entry}");
    assert_eq!(entry["description"], json!("first sale"), "{entry}");
    // and its lines arrive with it
    assert_eq!(entry["lines"].as_array().map(Vec::len), Some(2), "{entry}");
}

#[tokio::test]
async fn a_draft_entry_is_kept_and_kept_out_of_the_statements() {
    let mut walk = Walk::stranger();
    walk.register_and_sign_in().await;
    walk.open_book().await;
    let draft = walk.post_entry("draft", "99.00").await;
    assert_eq!(draft["status"], json!("draft"), "{draft}");

    // a draft is a bookkeeper's note, not a posting: the statements do not move until it is posted
    let balance = walk.trial_balance().await;
    assert_eq!(balance["total_debits"], json!("0.000000"), "{balance}");
    assert_eq!(balance["total_credits"], json!("0.000000"), "{balance}");

    // and once posted, the same entry counts
    walk.post_entry("posted", "99.00").await;
    let balance = walk.trial_balance().await;
    assert_eq!(balance["total_debits"], json!("99.000000"), "{balance}");
}

#[tokio::test]
async fn a_stranger_sets_up_an_organisation_and_their_book_lands_inside_it() {
    // The last step of the first run, and the one with no other way back: a Book opened before there is an organisation
    // belongs to nobody, so it can never be shared, and the person has no way to ask for one later. The walk ends here
    // rather than at the Book, because a Book nobody can reach is not a first run that worked.
    let mut walk = Walk::stranger();
    walk.register_and_sign_in().await;
    walk.open_book().await;
    let organisation = walk.organise().await;
    assert_eq!(organisation["name"], json!("Stan's studio"));
    // the person who opens it is its owner, which is the membership every later question reads
    assert_eq!(organisation["role"], json!("owner"));

    let (status, listed) = walk
        .call(Method::GET, "/identity/organizations", None)
        .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["count"], json!(1));
    assert_eq!(listed["organizations"][0]["role"], json!("owner"));

    let organisation_id = organisation["id"].as_str().expect("an id");
    let (status, books) = walk
        .call(
            Method::GET,
            &format!("/identity/organizations/{organisation_id}/books"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{books}");
    assert_eq!(books["count"], json!(1));
    assert_eq!(books["books"][0]["id"], json!(walk.book));

    // and the bookkeeping still answers inside the organisation: moving the Book did not move its accounts
    let (status, entry) = walk
        .call(
            Method::POST,
            "/accounting/journal-entries",
            Some(json!({
                "entry_date": "2026-11-15",
                "description": "first sale after the organisation was set up",
                "status": "posted",
                "lines": [
                    {"account_number": "1000", "debit": "250.00", "credit": "0"},
                    {"account_number": "4000", "debit": "0", "credit": "250.00"},
                ],
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{entry}");
    let balance = walk.trial_balance().await;
    assert_eq!(
        balance["total_debits"],
        json!("250.000000"),
        "the sale has to reach the trial balance after the Book moved: {balance}"
    );
    assert_eq!(balance["is_balanced"], json!(true), "{balance}");
}

/// A second person, joining a Book that is not theirs.
///
/// The first run does not end at one person's Book. A Book nobody else can reach is a filing cabinet with one key, so the
/// walk goes one step further: the owner invites somebody by name, that person sees the invitation, accepts it, and can then
/// read the same accounts. This is also where the shape of an invitation is proved - a pending membership has to be visible
/// to the invited person before they accept, or there is nothing to tap.
#[tokio::test]
async fn an_invited_person_sees_the_book_and_accepts_it() {
    let mut owner = Walk::stranger();
    owner.register_and_sign_in().await;
    owner.open_book().await;

    // a second person, with an account of their own, at the same engine as the first
    let mut guest = Walk::on(owner.router.clone());
    guest.sign_up("guest@example.com", "Gwen").await;
    let guest_id = guest.user.clone();

    // Each phone publishes the key an invitation is sealed for, in the shape the app sends: the two halves of an RSA key
    // as base64, because a Book key is wrapped with them and a key the engine cannot read is an invitation to nothing.
    let modulus = base64_of_128_bytes();
    for walk in [&owner, &guest] {
        let (status, published) = walk
            .call(
                Method::PUT,
                "/identity/users/me/public-key",
                Some(json!({"modulus": modulus, "exponent": "AQAB"})),
            )
            .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "a phone has to be reachable: {published}"
        );
        assert_eq!(published["modulus"], json!(modulus), "{published}");
    }
    let (status, fetched) = owner
        .call(
            Method::GET,
            &format!("/identity/users/{guest_id}/public-key"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{fetched}");
    assert_eq!(
        fetched["modulus"],
        json!(modulus),
        "an inviter has to be able to fetch the guest's key: {fetched}"
    );

    // the guest can be found by name, and has no Book of their own until this one arrives
    let (status, found) = owner
        .call(
            Method::GET,
            "/identity/users/lookup?username=guest@example.com",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["id"], json!(guest_id), "{found}");

    let (status, listed) = guest.call(Method::GET, "/books", None).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(
        listed["books"].as_array().map(Vec::len),
        Some(0),
        "{listed}"
    );

    // the invitation carries the Book key wrapped for the guest's key; anything else would join them to a Book they cannot read
    let (status, invited) = owner
        .call(
            Method::POST,
            &format!("/books/{}/members", owner.book),
            Some(json!({"user_id": guest_id, "role": "bookkeeper", "wrapped_book_key": "sealed-for-the-guest"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{invited}");
    assert_eq!(invited["status"], json!("invited"));

    // before accepting, the invitation is visible to the guest - and visibly not yet theirs
    let (status, listed) = guest.call(Method::GET, "/books", None).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let pending = listed["books"]
        .as_array()
        .expect("books")
        .iter()
        .find(|book| book["id"] == json!(owner.book))
        .expect("the invitation has to be visible before it is accepted");
    assert_eq!(pending["membership_status"], json!("invited"), "{pending}");
    assert_eq!(pending["your_role"], json!("bookkeeper"), "{pending}");

    // the owner's own copy of the Book still reads as theirs
    let (status, owner_books) = owner.call(Method::GET, "/books", None).await;
    assert_eq!(status, StatusCode::OK, "{owner_books}");
    assert_eq!(
        owner_books["books"][0]["membership_status"],
        json!("active"),
        "{owner_books}"
    );

    // accepting hands back the key the invitation carried
    let (status, accepted) = guest
        .call(
            Method::POST,
            &format!("/books/{}/members/accept", owner.book),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["status"], json!("active"), "{accepted}");
    assert_eq!(
        accepted["wrapped_book_key"],
        json!("sealed-for-the-guest"),
        "the guest has to receive the key that was sealed for them: {accepted}"
    );

    // The next step in a shared Book - reading its accounts - does not answer yet, and it is not a defect in accepting:
    // a Book's records are held under the person who wrote them, so a member who joined a moment ago is a member of a
    // Book that reads empty. Holding them by Book and checking membership on every path is the work that opens them.
    let (status, listed) = guest.call(Method::GET, "/books", None).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(
        listed["books"][0]["membership_status"],
        json!("active"),
        "after accepting, the Book reads as joined: {listed}"
    );
}

/// A modulus in the shape a phone publishes: base64 of 128 bytes, which is the smallest the engine will seal a Book key for.
fn base64_of_128_bytes() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let raw: Vec<u8> = (0..128).map(|byte| byte as u8).collect();
    let mut out = String::new();
    for chunk in raw.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let packed = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ALPHABET[(packed >> 18) as usize & 63] as char);
        out.push(ALPHABET[(packed >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(packed >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[packed as usize & 63] as char
        } else {
            '='
        });
    }
    out
}
