//! A graph that records what it was asked and answers from a script. Tests only.

use std::cell::RefCell;
use std::sync::Mutex;

use serde_json::{json, Value};

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

/// One stored node.
#[derive(Clone)]
struct Node {
    label: String,
    props: Value,
}

/// A graph that keeps its records in memory, so a test can follow one through its whole life: created, read,
/// listed, updated. The store's statements are few and fixed in shape, so this understands exactly those and
/// refuses anything else rather than guessing. Records are scoped the way the real store scopes them, which is
/// what lets a test show that one Book cannot see another's.
#[derive(Default)]
pub struct MemoryGraph {
    nodes: Mutex<Vec<Node>>,
}

/// The label a statement creates, read out of `CREATE (x:Label)`.
fn created_label(cypher: &str) -> Option<String> {
    let start = cypher.find("CREATE (x:")? + "CREATE (x:".len();
    let rest = &cypher[start..];
    let end = rest.find(')')?;
    Some(rest[..end].to_string())
}

/// Every label a statement matches, read out of `(x:Label)`.
fn matched_label(cypher: &str) -> Option<String> {
    let start = cypher.find("->(x:")? + "->(x:".len();
    let rest = &cypher[start..];
    let end = rest.find(')')?;
    Some(rest[..end].to_string())
}

fn stated(params: &Value, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

impl MemoryGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// The records a Book holds under one label, newest first, as the store's listing orders them.
    pub fn held(&self, book_id: &str, label: &str) -> Vec<Value> {
        let mut rows: Vec<Value> = self
            .nodes
            .lock()
            .expect("the memory graph is never poisoned")
            .iter()
            .filter(|node| {
                node.label == label
                    && node.props.get("book_id").and_then(Value::as_str) == Some(book_id)
            })
            .map(|node| node.props.clone())
            .collect();
        rows.sort_by_key(|row| {
            std::cmp::Reverse(
                row.get("created_at")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )
        });
        rows
    }
}

impl Graph for MemoryGraph {
    fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError> {
        // a create carries its own properties and stamps the caller's scope
        if cypher.contains("CREATE (x:") {
            let label = created_label(cypher)
                .ok_or_else(|| StoreError::Graph("no label in a create".into()))?;
            let props = params
                .get("props")
                .cloned()
                .ok_or_else(|| StoreError::Graph("a create with no properties".into()))?;
            self.nodes
                .lock()
                .expect("the memory graph is never poisoned")
                .push(Node {
                    label,
                    props: props.clone(),
                });
            return Ok(vec![json!({ "x": props })]);
        }

        let label = matched_label(cypher)
            .ok_or_else(|| StoreError::Graph(format!("unreadable statement: {cypher}")))?;
        let user_id = stated(&params, "user_id");
        let book_id = stated(&params, "book_id");
        let id = stated(&params, "id");

        // the caller's own records under this label
        let mut mine: Vec<usize> = Vec::new();
        for (index, node) in self
            .nodes
            .lock()
            .expect("the memory graph is never poisoned")
            .iter()
            .enumerate()
        {
            if node.label != label {
                continue;
            }
            if node.props.get("book_id").and_then(Value::as_str) != Some(book_id.as_str()) {
                continue;
            }
            if node.props.get("user_id").and_then(Value::as_str) != Some(user_id.as_str()) {
                continue;
            }
            mine.push(index);
        }

        if cypher.contains("DETACH DELETE") {
            let matching: Vec<usize> = mine
                .into_iter()
                .filter(|index| {
                    let nodes = self
                        .nodes
                        .lock()
                        .expect("the memory graph is never poisoned");
                    nodes[*index].props.get("id").and_then(Value::as_str) == Some(id.as_str())
                })
                .collect();
            if matching.is_empty() {
                return Ok(Vec::new());
            }
            let mut nodes = self
                .nodes
                .lock()
                .expect("the memory graph is never poisoned");
            for index in matching.into_iter().rev() {
                nodes.remove(index);
            }
            return Ok(vec![json!({"deleted": 1})]);
        }

        if cypher.contains("SET x += $patch") {
            let patch = params
                .get("patch")
                .cloned()
                .ok_or_else(|| StoreError::Graph("an update with no patch".into()))?;
            let mut found: Option<Value> = None;
            let mut nodes = self
                .nodes
                .lock()
                .expect("the memory graph is never poisoned");
            for index in mine {
                if nodes[index].props.get("id").and_then(Value::as_str) != Some(id.as_str()) {
                    continue;
                }
                if let (Some(object), Some(changes)) =
                    (nodes[index].props.as_object_mut(), patch.as_object())
                {
                    for (key, value) in changes {
                        object.insert(key.clone(), value.clone());
                    }
                }
                found = Some(nodes[index].props.clone());
            }
            return Ok(found
                .map(|props| vec![json!({ "x": props })])
                .unwrap_or_default());
        }

        if cypher.contains("WHERE x.id = $id") {
            let nodes = self
                .nodes
                .lock()
                .expect("the memory graph is never poisoned");
            let found = mine
                .into_iter()
                .find(|index| {
                    nodes[*index].props.get("id").and_then(Value::as_str) == Some(id.as_str())
                })
                .map(|index| nodes[index].props.clone());
            return Ok(found
                .map(|props| vec![json!({ "x": props })])
                .unwrap_or_default());
        }

        // a listing: the records under this label, newest first, capped by the stated limit
        let mut rows = self.held(&book_id, &label);
        let limit = params
            .get("limit")
            .and_then(Value::as_i64)
            .unwrap_or(i64::MAX);
        rows.truncate(limit.max(0) as usize);
        Ok(rows
            .into_iter()
            .map(|props| json!({ "x": props }))
            .collect())
    }
}
