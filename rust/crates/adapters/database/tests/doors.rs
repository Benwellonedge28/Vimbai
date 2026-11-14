//! A backend that speaks no query language at all, and still serves every caller.
//!
//! This is the point of the semantic doors: `Records` asks a store to create a record in a Book, not to run a statement. A
//! store that refuses `run` outright and answers only the doors is therefore a complete, working backend - which is what a
//! Postgres implementation has to be. If any caller still reached for `run`, this test would fail instead of the refactor
//! quietly leaving Cypher in the call path.

#![forbid(unsafe_code)]

use std::cell::RefCell;

use serde_json::{json, Map, Value};

use vimbai_store::{
    BookScope, Graph, PlatformRecords, RecordKind, Records, StoreError, EXPENSE, JOURNAL_ENTRY,
    JOURNAL_LINE,
};

/// A store with no query language: it holds records in a map and answers the doors, refusing everything else.
#[derive(Default)]
struct RecordsOnly {
    kinds: RefCell<Vec<(String, Map<String, Value>)>>,
    refused: RefCell<usize>,
}

impl RecordsOnly {
    fn refused(&self) -> usize {
        *self.refused.borrow()
    }
}

impl Graph for RecordsOnly {
    fn run(&self, _cypher: &str, _params: Value) -> Result<Vec<Value>, StoreError> {
        *self.refused.borrow_mut() += 1;
        Err(StoreError::Graph(
            "this store does not read statements".into(),
        ))
    }

    fn store_create(
        &self,
        kind: RecordKind,
        _scope: &BookScope,
        props: Value,
    ) -> Result<Value, StoreError> {
        self.kinds.borrow_mut().push((
            kind.label.to_string(),
            props.as_object().cloned().unwrap_or_default(),
        ));
        Ok(props)
    }

    fn store_create_with_children(
        &self,
        kind: RecordKind,
        _scope: &BookScope,
        props: Value,
        child: RecordKind,
        children: Vec<Value>,
    ) -> Result<Value, StoreError> {
        self.kinds.borrow_mut().push((
            kind.label.to_string(),
            props.as_object().cloned().unwrap_or_default(),
        ));
        for line in children {
            self.kinds.borrow_mut().push((
                child.label.to_string(),
                line.as_object().cloned().unwrap_or_default(),
            ));
        }
        let mut answer = props;
        answer["lines"] = json!(self.kinds.borrow().len());
        Ok(answer)
    }

    fn store_create_platform(&self, kind: RecordKind, props: Value) -> Result<Value, StoreError> {
        self.kinds.borrow_mut().push((
            kind.label.to_string(),
            props.as_object().cloned().unwrap_or_default(),
        ));
        Ok(props)
    }
}

#[test]
fn a_book_record_is_written_without_a_single_statement() {
    let store = RecordsOnly::default();
    let scope = BookScope::new("sam", "book-1");

    let stored = Records::new(&store, EXPENSE)
        .create(
            &scope,
            &json!({"amount": "12.50", "account_number": "5000"}),
        )
        .expect("a create without a statement");

    assert_eq!(
        stored.get("book_id").and_then(Value::as_str),
        Some("book-1")
    );
    assert!(
        stored.get("id").and_then(Value::as_str).is_some(),
        "the door did not stamp an id"
    );
    assert_eq!(
        store.refused(),
        0,
        "a caller fell back to running a statement"
    );
}

#[test]
fn an_entry_and_its_lines_are_written_without_a_single_statement() {
    let store = RecordsOnly::default();
    let scope = BookScope::new("sam", "book-1");

    Records::new(&store, JOURNAL_ENTRY)
        .create_with_children(
            &scope,
            &json!({"reference": "INV-1", "posted": true}),
            JOURNAL_LINE,
            &[
                json!({"account_number": "5000", "debit": "10", "credit": "0", "side": "debit"}),
                json!({"account_number": "1100", "debit": "0", "credit": "10", "side": "credit"}),
            ],
            "entry_id",
        )
        .expect("a posting without a statement");

    let held = store.kinds.borrow();
    assert_eq!(held.len(), 3, "the entry and its two lines: {held:?}");
    let entry_id = held[0]
        .1
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    for line in &held[1..] {
        assert_eq!(
            line.1.get("entry_id").and_then(Value::as_str),
            Some(entry_id),
            "a line did not name the entry it belongs to"
        );
        assert!(
            line.1.get("id").and_then(Value::as_str).is_some(),
            "the store assigns a line's id, not the payload"
        );
    }
    assert_eq!(store.refused(), 0);
}

#[test]
fn a_platform_record_is_written_without_a_single_statement() {
    let store = RecordsOnly::default();
    let account = RecordKind::new("Account", "OWNS_ACCOUNT");

    PlatformRecords::new(&store, account)
        .create(&json!({"id": "account-1", "email": "sam@example.test"}))
        .expect("an account without a statement");

    assert_eq!(store.refused(), 0);
    assert_eq!(store.kinds.borrow()[0].0, "Account");
}

#[test]
fn a_payload_still_cannot_choose_its_own_book() {
    // the doors carry the scope, so this remains the caller's refusal and not the backend's good behaviour
    let store = RecordsOnly::default();
    let scope = BookScope::new("sam", "book-1");
    let refused = Records::new(&store, EXPENSE)
        .create(&scope, &json!({"book_id": "someone-elses-book"}))
        .expect_err("a payload chose its own Book");
    assert!(refused.to_string().contains("book_id"), "{refused}");
}
