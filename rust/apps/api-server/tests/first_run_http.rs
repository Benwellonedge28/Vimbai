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
    /// A stranger, with no account and no Book, at the app the binary, the on-device runtime and the cluster all serve.
    ///
    /// The full app rather than the record routes alone, because the record routes are only half of it: the membership gate
    /// stands in front of them, and a walk built without it passed while a stranger could read another person's Book. A
    /// walk that is not the served app proves only that a router nobody runs answers what it was asked.
    fn stranger() -> Self {
        let graph: vimbai_api::records::GraphHandle = Arc::new(MemoryGraph::new());
        Self::on(vimbai_api::routed(Some(graph)))
    }

    /// The same app over the store an on-device install boots: a Book shared between two people has to work on a file, not
    /// only in the in-memory double the other walks use.
    fn on_disk(path: &std::path::Path) -> Self {
        let graph: vimbai_api::records::GraphHandle =
            Arc::new(store::file::FileGraph::open(path).expect("a file store"));
        Self::on(vimbai_api::routed(Some(graph)))
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

    /// A posting whose refusal is expected: `post_entry` insists on success, which is right everywhere except in a test about
    /// somebody who is not allowed to post.
    async fn post_try(&self, status: &str, amount: &str) -> (StatusCode, Value) {
        self.call(
            Method::POST,
            "/accounting/journal-entries",
            Some(json!({
                "entry_date": "2026-11-15",
                "description": "not allowed",
                "status": status,
                "lines": [
                    {"account_number": "1000", "debit": amount, "credit": "0"},
                    {"account_number": "4000", "debit": "0", "credit": amount},
                ],
            })),
        )
        .await
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

    // and the Book answers them, which is the point of accepting: a Book holds its records rather than each person holding
    // their own, so the accounts the owner seeded are the accounts the guest reads
    guest.book = owner.book.clone();
    let shared = guest.chart().await;
    let owned = owner.chart().await;
    assert!(!shared.is_empty(), "a joined Book has a chart: {shared:?}");
    assert_eq!(
        shared.len(),
        owned.len(),
        "a joined member reads the same chart as the owner"
    );

    // Reading is half of being in a Book. The guest was invited as a bookkeeper, so their posting has to land in the same
    // Book the owner reads - one set of books, two people keeping it. A Book that answered its members with the owner's
    // records but filed their postings somewhere else would look right on both screens and never add up.
    let posted = guest.post_entry("posted", "125.00").await;
    assert_eq!(posted["status"], json!("posted"), "{posted}");
    let owner_balance = owner.trial_balance().await;
    let guest_balance = guest.trial_balance().await;
    assert_eq!(
        owner_balance["total_debits"], guest_balance["total_debits"],
        "the owner and the member are reading one Book: {owner_balance} / {guest_balance}"
    );
    assert_ne!(
        owner_balance["total_debits"],
        json!("0.00"),
        "the member's posting moved the owner's statements: {owner_balance}"
    );

    // A Book id is not a secret, so naming one is not the same as being allowed in: somebody who never joined is refused
    // rather than shown an empty chart, which would be a lie about what the Book holds. The same goes for writing: a stranger
    // who could post into a Book would be able to change somebody's statements without ever being let in.
    let mut gatecrasher = Walk::on(owner.router.clone());
    gatecrasher.sign_up("gatecrasher@example.com", "Gwen").await;
    gatecrasher.book = owner.book.clone();
    let (status, refused) = gatecrasher
        .call(Method::GET, "/accounting/accounts", None)
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a Book opens for its members and nobody else: {refused}"
    );
    let (status, refused) = gatecrasher
        .call(
            Method::POST,
            "/accounting/journal-entries",
            Some(json!({
                "entry_date": "2026-11-15",
                "description": "not mine to write",
                "status": "posted",
                "lines": [
                    {"account_number": "1000", "debit": "5.00", "credit": "0"},
                    {"account_number": "4000", "debit": "0", "credit": "5.00"},
                ],
            })),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a Book takes entries from its members and nobody else: {refused}"
    );
    // and the refusal left nothing behind
    let after = owner.trial_balance().await;
    assert_eq!(
        after["total_debits"], owner_balance["total_debits"],
        "a refused posting must not move the statements: {after}"
    );

    let (status, listed) = guest.call(Method::GET, "/books", None).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(
        listed["books"][0]["membership_status"],
        json!("active"),
        "after accepting, the Book reads as joined: {listed}"
    );
}

#[tokio::test]
async fn a_book_shared_on_disk_takes_everybodys_postings_and_keeps_them() {
    // The in-memory double is what most walks run against, and it was stricter than the stores the product uses - it answered
    // a Book read as a personal read - so a shared Book's postings were invisible to the test and right in the product. This
    // walk runs on the store an on-device install boots, and reopens it, so the claim is about the file and not about a fake.
    let dir = std::env::temp_dir().join(format!("vimbai-shared-book-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let path = dir.join("book.json");

    let mut owner = Walk::on_disk(&path);
    owner.register_and_sign_in().await;
    owner.open_book().await;

    let mut guest = Walk::on(owner.router.clone());
    // sign_up answers the session it signed in with; the invitation names the person, so the id is read from the walk
    guest.sign_up("guest@example.com", "Gus").await;
    let guest_id = guest.user.clone();
    let (status, _) = guest
        .call(
            Method::PUT,
            "/identity/users/me/public-key",
            Some(json!({"modulus": base64_of_128_bytes(), "exponent": "AQAB"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, invited) = owner
        .call(
            Method::POST,
            &format!("/books/{}/members", owner.book),
            Some(json!({"user_id": guest_id, "role": "bookkeeper", "wrapped_book_key": "sealed-for-the-guest"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{invited}");
    let (status, accepted) = guest
        .call(
            Method::POST,
            &format!("/books/{}/members/accept", owner.book),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");

    // the member posts, and the owner's statements move: this is the half of sharing a member actually needs
    guest.book = owner.book.clone();
    guest.post_entry("posted", "240.00").await;
    let owner_balance = owner.trial_balance().await;
    assert_ne!(
        owner_balance["total_debits"],
        json!("0.000000"),
        "a member's posting has to land in the Book they were invited to: {owner_balance}"
    );

    // and it is on the file: a second engine opened on the same path reads the same statements
    drop(guest);
    let reopened = Walk::on_disk(&path);
    let mut reader = Walk {
        router: reopened.router.clone(),
        user: owner.user.clone(),
        token: owner.token.clone(),
        book: owner.book.clone(),
    };
    let after = reader.trial_balance().await;
    assert_eq!(
        after["total_debits"], owner_balance["total_debits"],
        "a shared Book's postings survive the store being opened again: {after}"
    );
    reader.book.clear();

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_member_whose_phone_cannot_read_the_book_can_be_given_the_key_again() {
    // The failure this covers is the one a person actually meets: they accept an invitation on a phone whose key the invitation
    // was not sealed for, or they replace the phone a month later. The membership is real and the key is theirs, but there is
    // nothing on their device to open. The advice the app gives is "ask whoever shared it to invite you again" - which used to be
    // impossible to follow, because inviting somebody already in the Book was refused as a conflict and accepting a second time
    // was refused as not an invitation. Both halves have to work, or the sentence is a dead end.
    let mut owner = Walk::stranger();
    owner.register_and_sign_in().await;
    owner.open_book().await;

    let mut guest = Walk::on(owner.router.clone());
    guest.sign_up("guest@example.com", "Gus").await;
    let guest_id = guest.user.clone();

    let (status, invited) = owner
        .call(
            Method::POST,
            &format!("/books/{}/members", owner.book),
            Some(
                json!({"user_id": guest_id, "role": "bookkeeper", "wrapped_book_key": "first-key"}),
            ),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{invited}");

    let (status, accepted) = guest
        .call(
            Method::POST,
            &format!("/books/{}/members/accept", owner.book),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(
        accepted["wrapped_book_key"],
        json!("first-key"),
        "{accepted}"
    );

    // the owner sends the key again, for the new phone. The invitation is re-issued rather than refused, and the person's
    // standing in the Book is untouched: a key is not a promotion.
    let (status, again) = owner
        .call(
            Method::POST,
            &format!("/books/{}/members", owner.book),
            Some(json!({"user_id": guest_id, "role": "viewer", "wrapped_book_key": "second-key"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["status"], json!("active"), "{again}");
    assert_eq!(
        again["role"],
        json!("bookkeeper"),
        "a member already in the Book keeps the role they hold: {again}"
    );

    // and the member collects it themselves, without needing a second invitation accepted
    let (status, key) = guest
        .call(Method::GET, &format!("/books/{}/key", owner.book), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{key}");
    assert_eq!(
        key["wrapped_book_key"],
        json!("second-key"),
        "the member asks for the key that is theirs and gets the newest one: {key}"
    );

    // a Book's key is not something a stranger can ask for by naming the Book
    let mut gatecrasher = Walk::on(owner.router.clone());
    gatecrasher.sign_up("gatecrasher@example.com", "Gwen").await;
    gatecrasher.book = owner.book.clone();
    let (status, refused) = gatecrasher
        .call(Method::GET, &format!("/books/{}/key", owner.book), None)
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the key door opens for members: {refused}"
    );
}

#[tokio::test]
async fn somebody_taken_out_of_a_book_cannot_read_or_post_to_it() {
    // Sharing has to be reversible or it is a one-way door: an owner who brings in the wrong person, or a bookkeeper who stops
    // working for them, needs the access to stop. Revoking is the engine's word for that, and what has to be true afterwards is
    // simple - the Book answers them with a refusal, and nothing they post lands in it.
    let mut owner = Walk::stranger();
    owner.register_and_sign_in().await;
    owner.open_book().await;

    let mut guest = Walk::on(owner.router.clone());
    guest.sign_up("guest@example.com", "Gus").await;
    let guest_id = guest.user.clone();

    let (status, invited) = owner
        .call(
            Method::POST,
            &format!("/books/{}/members", owner.book),
            Some(json!({"user_id": guest_id, "role": "bookkeeper", "wrapped_book_key": "sealed-for-the-guest"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{invited}");
    let (status, _) = guest
        .call(
            Method::POST,
            &format!("/books/{}/members/accept", owner.book),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    guest.book = owner.book.clone();
    guest.post_entry("posted", "80.00").await;
    let before = owner.trial_balance().await;

    // the owner takes them out
    let (status, revoked) = owner
        .call(
            Method::PUT,
            &format!("/books/{}/members/{guest_id}", owner.book),
            Some(json!({"status": "revoked"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{revoked}");

    // the Book is closed to them: the chart, the statements they were reading a moment ago, and the posting they might try
    let (status, refused) = guest.call(Method::GET, "/accounting/accounts", None).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a revoked member is not a member: {refused}"
    );
    let (status, refused) = guest.post_try("posted", "999.00").await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a revoked member cannot post into the Book: {refused}"
    );

    // and nothing changed for the people still in it
    let after = owner.trial_balance().await;
    assert_eq!(
        after["total_debits"], before["total_debits"],
        "a refused posting leaves the statements alone: {after}"
    );

    // the owner's own membership cannot be revoked by way of this door, which is what stops a Book being orphaned
    let owner_id = owner.user.clone();
    let (status, refused) = owner
        .call(
            Method::PUT,
            &format!("/books/{}/members/{owner_id}", owner.book),
            Some(json!({"status": "revoked"})),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "the owner of a Book is not removed from it: {refused}"
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
