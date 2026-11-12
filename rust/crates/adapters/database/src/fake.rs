//! A graph that records what it was asked and answers from a script. Tests only.

use std::cell::RefCell;

use serde_json::Value;

use crate::{Graph, StoreError};

pub struct FakeGraph {
    rows: RefCell<Vec<Value>>,
    /// When set, each statement takes its own script instead of the shared rows, so a path that runs
    /// several queries (a summary that reads three collections) can be tested as a whole.
    scripts: RefCell<Vec<Vec<Value>>>,
    seen: RefCell<Vec<(String, Value)>>,
}

impl FakeGraph {
    pub fn returning(rows: Vec<Value>) -> Self {
        Self {
            rows: RefCell::new(rows),
            scripts: RefCell::new(Vec::new()),
            seen: RefCell::new(Vec::new()),
        }
    }

    /// One script per statement, in order. The last script answers every later statement, so a test
    /// only has to spell out as many answers as it cares about.
    pub fn queue(scripts: Vec<Vec<Value>>) -> Self {
        Self {
            rows: RefCell::new(Vec::new()),
            scripts: RefCell::new(scripts),
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

    /// The nth statement the code ran, in order, for a path whose first query matters as much as
    /// its last.
    pub fn statement(&self, index: usize) -> (String, Value) {
        self.seen
            .borrow()
            .get(index)
            .cloned()
            .expect("statement not run")
    }

    pub fn statements(&self) -> usize {
        self.seen.borrow().len()
    }
}

impl Graph for FakeGraph {
    fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError> {
        let index = self.seen.borrow().len();
        self.seen.borrow_mut().push((cypher.to_owned(), params));
        let scripts = self.scripts.borrow();
        if scripts.is_empty() {
            return Ok(self.rows.borrow().clone());
        }
        let last = scripts.len() - 1;
        Ok(scripts[index.min(last)].clone())
    }
}
