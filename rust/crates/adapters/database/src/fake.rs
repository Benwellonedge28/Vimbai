//! The engine's own graph, written out rather than spoken to a database.
//!
//! `FakeGraph` scripts answers for a test. `MemoryGraph` is the store's real statements held in memory, which is what a
//! test walks a record through and what the on-device runtime keeps on disk through `FileGraph`.

use std::cell::RefCell;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{BookScope, Graph, RecordKind, StoreError};

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

    // A scripted fake answers statements, not edges: it has nowhere to put them, and a test using it is asking about the
    // statements a caller writes rather than about traversal.
    fn all_edges(&self) -> Result<Vec<(String, String, String)>, StoreError> {
        Ok(Vec::new())
    }

    fn create_edge(&self, _from: &str, _label: &str, _to: &str) -> Result<(), StoreError> {
        Ok(())
    }
}

/// One stored node.
#[derive(Clone, Serialize, Deserialize)]
struct Node {
    label: String,
    props: Value,
}

/// A graph that keeps its records in memory, so a test can follow one through its whole life: created, read,
/// listed, updated. The store's statements are few and fixed in shape, so this understands exactly those and
/// refuses anything else rather than guessing. Records are scoped the way the real store scopes them, which is
/// what lets a test show that one Book cannot see another's.
/// One edge between two stored nodes, by the ids they carry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub label: String,
    pub to: String,
}

#[derive(Default)]
pub struct MemoryGraph {
    nodes: Mutex<Vec<Node>>,
    edges: Mutex<Vec<Edge>>,
}

/// The label a statement creates, read out of `CREATE (x:Label)`.
fn created_label(cypher: &str) -> Option<String> {
    let start = cypher.find("CREATE (x:")? + "CREATE (x:".len();
    let rest = &cypher[start..];
    let end = rest.find(')')?;
    Some(rest[..end].to_string())
}

/// Every label a statement matches, read out of `(x:Label)` - whether it hangs off the caller or the Book.
fn matched_label(cypher: &str) -> Option<String> {
    let start = match cypher.find("->(x:") {
        Some(found) => found + "->(x:".len(),
        None => cypher.find("MATCH (x:")? + "MATCH (x:".len(),
    };
    let rest = &cypher[start..];
    let end = rest.find(')')?;
    Some(rest[..end].to_string())
}

/// The property a statement filters on when it is not the id: `WHERE x.<property> = $value`.
fn filtered_property(cypher: &str) -> Option<String> {
    // the property is the one written immediately before `= $value`, so read back from there
    let end = cypher.find(" = $value")?;
    let before = &cypher[..end];
    let start = before.rfind("x.")? + 2;
    Some(before[start..].to_string())
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

    /// Everything held, as plain JSON, so it can be written to a file and read back.
    pub fn snapshot(&self) -> Value {
        let nodes: Vec<Value> = self
            .nodes
            .lock()
            .expect("the memory graph is never poisoned")
            .iter()
            .map(|node| json!({"label": node.label, "props": node.props}))
            .collect();
        let edges = self
            .edges
            .lock()
            .expect("the memory graph is never poisoned")
            .clone();
        json!({"nodes": nodes, "edges": edges})
    }

    /// Replace everything held with a snapshot taken by `snapshot`, which is how a file comes back as a graph.
    pub fn restore(&self, snapshot: &Value) {
        let nodes: Vec<Node> = snapshot
            .get("nodes")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| {
                        Some(Node {
                            label: row.get("label")?.as_str()?.to_string(),
                            props: row.get("props")?.clone(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let edges: Vec<Edge> = snapshot
            .get("edges")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        *self
            .nodes
            .lock()
            .expect("the memory graph is never poisoned") = nodes;
        *self
            .edges
            .lock()
            .expect("the memory graph is never poisoned") = edges;
    }

    /// Every edge of one label, oldest first, so a test can see what a write joined up.
    pub fn edges(&self, label: &str) -> Vec<Edge> {
        self.edges
            .lock()
            .expect("the memory graph is never poisoned")
            .iter()
            .filter(|edge| edge.label == label)
            .cloned()
            .collect()
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
    fn store_list_after(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        after: &str,
        limit: usize,
    ) -> Result<Vec<Value>, StoreError> {
        let nodes = self
            .nodes
            .lock()
            .expect("the memory graph is never poisoned");
        let mut rows: Vec<Value> = nodes
            .iter()
            .filter(|node| node.label == kind.label)
            .filter(|node| {
                node.props.get("book_id").and_then(Value::as_str) == Some(&scope.book_id)
            })
            .filter(|node| {
                node.props
                    .get("id")
                    .and_then(Value::as_str)
                    .map(|id| id > after)
                    .unwrap_or(false)
            })
            .map(|node| node.props.clone())
            .collect();
        rows.sort_by_key(|row| {
            row.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        });
        rows.truncate(limit);
        Ok(rows)
    }

    fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError> {
        // a create with children writes the parent and each child, and answers the parent with its lines
        if cypher.contains("CREATE (x:") && cypher.contains("UNWIND $children") {
            let label = created_label(cypher)
                .ok_or_else(|| StoreError::Graph("no label in a create".into()))?;
            let props = params
                .get("props")
                .cloned()
                .ok_or_else(|| StoreError::Graph("a create with no properties".into()))?;
            let child_label = cypher
                .find("CREATE (l:")
                .map(|start| {
                    let rest = &cypher[start + "CREATE (l:".len()..];
                    rest[..rest.find(')').expect("a label ends")].to_string()
                })
                .ok_or_else(|| StoreError::Graph("no child label in a create".into()))?;
            let children = params
                .get("children")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let mut nodes = self
                .nodes
                .lock()
                .expect("the memory graph is never poisoned");
            nodes.push(Node {
                label,
                props: props.clone(),
            });
            for child in &children {
                nodes.push(Node {
                    label: child_label.clone(),
                    props: child.clone(),
                });
            }
            drop(nodes);

            // the statement links each line to its parent, and to the account it moves by the side it states
            let parent_id = props
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let child_edge = cypher
                .find("MERGE (x)-[:")
                .map(|start| {
                    let rest = &cypher[start + "MERGE (x)-[:".len()..];
                    rest[..rest.find(']').unwrap_or(rest.len())].to_string()
                })
                .unwrap_or_default();
            let book = stated(&params, "book_id");
            {
                let mut edges = self
                    .edges
                    .lock()
                    .expect("the memory graph is never poisoned");
                for child in &children {
                    let line_id = child
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if !child_edge.is_empty() {
                        edges.push(Edge {
                            from: parent_id.clone(),
                            label: child_edge.clone(),
                            to: line_id.clone(),
                        });
                    }
                    let number = child
                        .get("account_number")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let side = child
                        .get("side")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let account = self
                        .nodes
                        .lock()
                        .expect("the memory graph is never poisoned")
                        .iter()
                        .find(|node| {
                            node.label == "Account"
                                && node.props.get("book_id").and_then(Value::as_str)
                                    == Some(book.as_str())
                                && node.props.get("account_number").and_then(Value::as_str)
                                    == Some(number)
                        })
                        .and_then(|node| {
                            node.props
                                .get("id")
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                        });
                    if let Some(account) = account {
                        let label = match side {
                            "debit" => "DEBITED",
                            "credit" => "CREDITED",
                            _ => "",
                        };
                        if !label.is_empty() {
                            edges.push(Edge {
                                from: line_id,
                                label: label.to_string(),
                                to: account,
                            });
                        }
                    }
                }
            }

            let mut answer = props;
            if let Some(object) = answer.as_object_mut() {
                object.insert("lines".into(), json!(children));
            }
            return Ok(vec![json!({ "x": answer })]);
        }

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

        // a read that walks a line to the account it moves: the edges decide, which is what makes them an index
        if cypher.contains("-[:DEBITED|CREDITED]->(a:Account") {
            let book = stated(&params, "book_id");
            let number = stated(&params, "account_number");
            let nodes = self
                .nodes
                .lock()
                .expect("the memory graph is never poisoned");
            let account = nodes
                .iter()
                .find(|node| {
                    node.label == "Account"
                        && node.props.get("book_id").and_then(Value::as_str) == Some(book.as_str())
                        && node.props.get("account_number").and_then(Value::as_str)
                            == Some(number.as_str())
                })
                .and_then(|node| {
                    node.props
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                });
            let Some(account) = account else {
                return Ok(Vec::new());
            };
            let touching: Vec<String> = self
                .edges
                .lock()
                .expect("the memory graph is never poisoned")
                .iter()
                .filter(|edge| {
                    edge.to == account && (edge.label == "DEBITED" || edge.label == "CREDITED")
                })
                .map(|edge| edge.from.clone())
                .collect();
            let mut rows: Vec<Value> = Vec::new();
            for line in nodes.iter().filter(|node| node.label == "JournalLine") {
                let id = line
                    .props
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !touching.iter().any(|touching| touching == id) {
                    continue;
                }
                let entry_id = line
                    .props
                    .get("entry_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let entry = nodes
                    .iter()
                    .find(|node| node.props.get("id").and_then(Value::as_str) == Some(entry_id));
                rows.push(json!({
                    "l": line.props.clone(),
                    "entry_id": entry_id,
                    "entry_date": entry.and_then(|e| e.props.get("entry_date")).cloned().unwrap_or(Value::Null),
                    "entry_description": entry.and_then(|e| e.props.get("description")).cloned().unwrap_or(Value::Null),
                }));
            }
            return Ok(rows);
        }

        // a read that collects children answers each parent with the lines that name it as their entry
        if cypher.contains("collect(l)") {
            let label = matched_label(cypher)
                .ok_or_else(|| StoreError::Graph(format!("unreadable statement: {cypher}")))?;
            let shared = params.get("user_id").is_none();
            let user_id = stated(&params, "user_id");
            let book_id = stated(&params, "book_id");
            let id = stated(&params, "id");
            let nodes = self
                .nodes
                .lock()
                .expect("the memory graph is never poisoned");
            let mine: Vec<&Node> = nodes
                .iter()
                .filter(|node| node.label == label)
                .filter(|node| {
                    params.get("book_id").is_none()
                        || node.props.get("book_id").and_then(Value::as_str)
                            == Some(book_id.as_str())
                })
                .filter(|node| {
                    shared
                        || node.props.get("user_id").and_then(Value::as_str)
                            == Some(user_id.as_str())
                })
                .filter(|node| {
                    id.is_empty()
                        || node.props.get("id").and_then(Value::as_str) == Some(id.as_str())
                })
                .collect();
            // the child label is named in the statement, so a child is any node of it that carries the parent's id
            let child_label = cypher
                .find("->(l:")
                .map(|start| {
                    let rest = &cypher[start + "->(l:".len()..];
                    rest[..rest.find(')').unwrap_or(rest.len())].to_string()
                })
                .unwrap_or_default();
            let mut rows: Vec<Value> = Vec::with_capacity(mine.len());
            for node in mine {
                let own = node
                    .props
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let lines: Vec<Value> = nodes
                    .iter()
                    .filter(|child| child.label == child_label)
                    .filter(|child| {
                        child
                            .props
                            .as_object()
                            .map(|fields| fields.values().any(|value| value.as_str() == Some(own)))
                            .unwrap_or(false)
                    })
                    .map(|child| child.props.clone())
                    .collect();
                let mut row = node.props.clone();
                if let Some(object) = row.as_object_mut() {
                    object.insert("lines".into(), Value::Array(lines));
                }
                rows.push(json!({ "x": row }));
            }
            return Ok(rows);
        }

        let label = matched_label(cypher)
            .ok_or_else(|| StoreError::Graph(format!("unreadable statement: {cypher}")))?;
        // a statement that states no user_id is a Book-keyed read (see the shared module): the Book is the owner,
        // and the caller has already established membership
        let shared = params.get("user_id").is_none();
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
            // a statement that states no Book is asking across Books, which only a person's own rows may do
            if params.get("book_id").is_some()
                && node.props.get("book_id").and_then(Value::as_str) != Some(book_id.as_str())
            {
                continue;
            }
            if !shared
                && node.props.get("user_id").and_then(Value::as_str) != Some(user_id.as_str())
            {
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

        if let Some(property) = filtered_property(cypher) {
            let wanted = params.get("value").cloned().unwrap_or(Value::Null);
            let nodes = self
                .nodes
                .lock()
                .expect("the memory graph is never poisoned");
            let found: Vec<Value> = mine
                .into_iter()
                .filter(|index| nodes[*index].props.get(&property) == Some(&wanted))
                .map(|index| nodes[index].props.clone())
                .collect();
            return Ok(found
                .into_iter()
                .map(|props| json!({ "x": props }))
                .collect());
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

        // a listing: the records under this label, newest first, capped by the stated limit.
        //
        // The rows a listing answers are the ones the scoping above selected - a caller's own records, or, for a read
        // that states no user, the Book's. Asking the Book for every row under the label instead would hand one person
        // another person's records, which is the one thing the store exists to prevent, and would have made every
        // isolation test that shared a Book pass while the isolation was absent.
        let _ = &book_id;
        let mut rows: Vec<Value> = {
            let nodes = self
                .nodes
                .lock()
                .expect("the memory graph is never poisoned");
            mine.iter()
                .map(|index| nodes[*index].props.clone())
                .collect()
        };
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

    fn all_edges(&self) -> Result<Vec<(String, String, String)>, StoreError> {
        Ok(self
            .edges
            .lock()
            .expect("the memory graph is never poisoned")
            .iter()
            .map(|edge| (edge.from.clone(), edge.label.clone(), edge.to.clone()))
            .collect())
    }

    fn create_edge(&self, from: &str, label: &str, to: &str) -> Result<(), StoreError> {
        // both ends have to be here: an edge to a record that was never written is a dangling read waiting to happen
        let known = |id: &str| {
            self.nodes
                .lock()
                .expect("the memory graph is never poisoned")
                .iter()
                .any(|node| node.props.get("id").and_then(Value::as_str) == Some(id))
        };
        if !known(from) || !known(to) {
            return Err(StoreError::Invalid(format!(
                "{from} -> {label} -> {to} names a record that is not stored"
            )));
        }
        self.edges
            .lock()
            .expect("the memory graph is never poisoned")
            .push(Edge {
                from: from.to_string(),
                label: label.to_string(),
                to: to.to_string(),
            });
        Ok(())
    }
}

#[cfg(test)]
mod scoping_tests {
    use super::*;
    use crate::{BookScope, RecordKind, Records};
    use serde_json::json;
    use serde_json::Value;

    const KIND: RecordKind = RecordKind::new("ScopedThing", "OWNS_SCOPED_THING");

    fn listing(graph: &MemoryGraph, scope: &BookScope) -> Vec<Value> {
        Records::new(graph, KIND)
            .list(scope, 100)
            .expect("a listing")
    }

    #[test]
    fn a_listing_answers_the_callers_own_rows_and_not_somebody_elses() {
        let graph = MemoryGraph::new();
        let mine = BookScope::new("user-1", "book-1");
        let theirs = BookScope::new("user-2", "book-1");
        Records::new(&graph, KIND)
            .create(&mine, &json!({"name": "mine"}))
            .expect("written");
        Records::new(&graph, KIND)
            .create(&theirs, &json!({"name": "theirs"}))
            .expect("written");

        // two people in the SAME Book: each sees only their own row. A listing that answered the Book's rows would hand
        // one person the other's records, which is the one thing the store exists to prevent
        let mine_listed = listing(&graph, &mine);
        let theirs_listed = listing(&graph, &theirs);
        assert_eq!(mine_listed.len(), 1);
        assert_eq!(theirs_listed.len(), 1);
        assert_eq!(mine_listed[0]["name"], json!("mine"));
        assert_eq!(theirs_listed[0]["name"], json!("theirs"));

        // and one person in another Book sees nothing of either
        assert!(listing(&graph, &BookScope::new("user-3", "book-2")).is_empty());
    }

    #[test]
    fn a_read_that_states_no_user_answers_the_books_rows() {
        // the one deliberate exception: a Book-keyed record set (see the shared module), where the Book owns the rows and
        // the caller has already established membership
        let graph = MemoryGraph::new();
        Records::new(&graph, KIND)
            .create(
                &BookScope::new("user-1", "book-1"),
                &json!({"name": "shared"}),
            )
            .expect("written");

        let rows = graph
            .run(
                "MATCH (x:ScopedThing) WHERE x.book_id = $book_id RETURN x LIMIT $limit",
                json!({"book_id": "book-1", "limit": 100}),
            )
            .expect("a listing");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["x"]["name"], json!("shared"));
        // and another Book's rows are still not visible
        let elsewhere = graph
            .run(
                "MATCH (x:ScopedThing) WHERE x.book_id = $book_id RETURN x LIMIT $limit",
                json!({"book_id": "book-2", "limit": 100}),
            )
            .expect("a listing");
        assert!(elsewhere.is_empty());
    }
}
