//! A graph that records what it was asked and answers from a script. Tests only.

use std::cell::RefCell;

use serde_json::Value;

use crate::{Graph, StoreError};

pub struct FakeGraph {
    rows: RefCell<Vec<Value>>,
    seen: RefCell<Vec<(String, Value)>>,
}

impl FakeGraph {
    pub fn returning(rows: Vec<Value>) -> Self {
        Self {
            rows: RefCell::new(rows),
            seen: RefCell::new(Vec::new()),
        }
    }

    pub fn last(&self) -> (String, Value) {
        self.seen
            .borrow()
            .last()
            .cloned()
            .expect("no statement ran")
    }

    pub fn statements(&self) -> usize {
        self.seen.borrow().len()
    }
}

impl Graph for FakeGraph {
    fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError> {
        self.seen.borrow_mut().push((cypher.to_owned(), params));
        Ok(self.rows.borrow().clone())
    }
}
