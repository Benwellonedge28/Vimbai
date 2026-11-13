#![forbid(unsafe_code)]
//! Records whose owner is the Book rather than the person.
//!
//! Every other kind in this store hangs off a person: the caller's id is written on the record, and a read only finds
//! records carrying both the caller and their Book. That is the right default, and it is what keeps one Book's rows out
//! of another's answer.
//!
//! A *shared* Book breaks that default. Its members are different people, and each of them has to read what the others
//! wrote: an entry one device pushes has to reach every other device, a membership one admin invites has to be found by
//! the person invited, metadata one treasurer stores has to be pulled by the household. Keying those rows on the caller
//! would mean either copying every row per person or hiding the work behind a superuser.
//!
//! So this is the second keying, and it is deliberately narrow:
//!
//! * rows are keyed by `book_id` alone, and that is the only filter a read applies;
//! * **the caller must have checked membership first.** Nothing here checks anything. That is the whole contract, and it
//!   is why this type is used by exactly the services that gate on a membership and by nothing else;
//! * no row may carry `user_id` as its scope. Where a person matters - who wrote an entry, who is invited - the person
//!   is an ordinary property with its own name (`author_user_id`, `member_user_id`), so a row can say who did something
//!   without becoming theirs alone.
//!
//! The Book stays the atomic unit, which is what the architecture asks for: a Book's contents belong to the Book, and a
//! person's right to see them comes from their membership.

use serde_json::{json, Map, Value};

use crate::{unwrap_node, Graph, RecordKind, StoreError};

/// Book-keyed records: the shape a shared Book's members read from.
pub struct SharedRecords<'a, G: Graph + ?Sized> {
    graph: &'a G,
    kind: RecordKind,
}

impl<'a, G: Graph + ?Sized> SharedRecords<'a, G> {
    pub fn new(graph: &'a G, kind: RecordKind) -> Self {
        Self { graph, kind }
    }

    /// Store a new row in a Book. The caller has already established that this person belongs to the Book.
    pub fn create(&self, book_id: &str, fields: &Value) -> Result<Value, StoreError> {
        let mut props = clean(fields)?;
        props.remove("book_id");
        props.insert("book_id".into(), json!(book_id));
        let id = props
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(vimbai_common::ids::service_id);
        props.insert("id".into(), json!(id));
        let cypher = format!(
            "CREATE (x:{label})\nSET x = $props\nRETURN x",
            label = self.kind.label
        );
        self.one(cypher, json!({ "props": Value::Object(props) }))
    }

    /// One row of a Book, by its own id.
    pub fn get(&self, book_id: &str, id: &str) -> Result<Value, StoreError> {
        let cypher = format!(
            "MATCH (x:{label})\nWHERE x.id = $id AND x.book_id = $book_id\nRETURN x",
            label = self.kind.label
        );
        self.one(cypher, json!({ "book_id": book_id, "id": id }))
    }

    /// A Book's rows, newest first.
    pub fn list(&self, book_id: &str, limit: usize) -> Result<Vec<Value>, StoreError> {
        let cypher = format!(
            "MATCH (x:{label})\nWHERE x.book_id = $book_id\nRETURN x\nORDER BY x.created_at DESC\nLIMIT $limit",
            label = self.kind.label
        );
        self.rows(cypher, json!({ "book_id": book_id, "limit": limit as i64 }))
    }

    /// A Book's rows whose named property equals the value given - the membership a person holds, the entry a device
    /// already sent.
    pub fn where_eq(
        &self,
        book_id: &str,
        property: &str,
        value: &Value,
    ) -> Result<Vec<Value>, StoreError> {
        let cypher = format!(
            "MATCH (x:{label})\nWHERE x.book_id = $book_id AND x.{property} = $value\nRETURN x",
            label = self.kind.label,
            property = property
        );
        self.rows(cypher, json!({ "book_id": book_id, "value": value }))
    }

    /// Rows of one property across every Book - used for "the Books I belong to", where the person is the filter.
    pub fn everywhere(&self, property: &str, value: &Value) -> Result<Vec<Value>, StoreError> {
        let cypher = format!(
            "MATCH (x:{label})\nWHERE x.{property} = $value\nRETURN x",
            label = self.kind.label,
            property = property
        );
        self.rows(cypher, json!({ "value": value }))
    }

    /// Merge fields into a row of this Book.
    pub fn update(&self, book_id: &str, id: &str, fields: &Value) -> Result<Value, StoreError> {
        let mut patch = clean(fields)?;
        patch.remove("book_id");
        let cypher = format!(
            "MATCH (x:{label})\nWHERE x.book_id = $book_id AND x.id = $id\nSET x += $patch\nRETURN x",
            label = self.kind.label
        );
        self.one(
            cypher,
            json!({ "book_id": book_id, "id": id, "patch": Value::Object(patch) }),
        )
    }

    /// Remove a row of this Book.
    pub fn delete(&self, book_id: &str, id: &str) -> Result<(), StoreError> {
        let cypher = format!(
            "MATCH (x:{label})\nWHERE x.book_id = $book_id AND x.id = $id\nDETACH DELETE x\nRETURN count(x) AS deleted",
            label = self.kind.label
        );
        self.graph
            .run(cypher.as_str(), json!({ "book_id": book_id, "id": id }))?;
        Ok(())
    }

    fn one(&self, cypher: impl AsRef<str>, params: Value) -> Result<Value, StoreError> {
        let rows = self.graph.run(cypher.as_ref(), params)?;
        rows.into_iter()
            .next()
            .map(unwrap_node)
            .ok_or(StoreError::NotFound)
    }

    fn rows(&self, cypher: impl AsRef<str>, params: Value) -> Result<Vec<Value>, StoreError> {
        Ok(self
            .graph
            .run(cypher.as_ref(), params)?
            .into_iter()
            .map(unwrap_node)
            .collect())
    }
}

/// A row must not claim to be someone's own; the Book is the owner here.
fn clean(fields: &Value) -> Result<Map<String, Value>, StoreError> {
    let object = fields
        .as_object()
        .ok_or_else(|| StoreError::Invalid("a record must be a JSON object".into()))?;
    if object.contains_key("user_id") {
        return Err(StoreError::Invalid(
            "a Book's rows name the person they concern (author_user_id, member_user_id), not user_id".into(),
        ));
    }
    Ok(object.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::MemoryGraph;

    const ROW: RecordKind = RecordKind::new("SharedRow", "OWNS_SHARED_ROW");

    #[test]
    fn rows_are_found_by_their_book_and_not_by_who_wrote_them() {
        let graph = MemoryGraph::new();
        let rows = SharedRecords::new(&graph, ROW);
        let first = rows
            .create(
                "book-1",
                &json!({"author_user_id": "user-1", "body": "one"}),
            )
            .expect("a row");
        rows.create(
            "book-1",
            &json!({"author_user_id": "user-2", "body": "two"}),
        )
        .expect("a row");
        rows.create(
            "book-2",
            &json!({"author_user_id": "user-1", "body": "elsewhere"}),
        )
        .expect("a row");

        // the whole Book reads back, whoever wrote what
        assert_eq!(rows.list("book-1", 10).expect("rows").len(), 2);
        assert_eq!(rows.list("book-2", 10).expect("rows").len(), 1);
        // and one row is reachable by its own id
        let held = rows
            .get("book-1", first["id"].as_str().expect("an id"))
            .expect("a row");
        assert_eq!(held["body"], json!("one"));
        // a row is not reachable through the wrong Book
        assert!(rows
            .get("book-2", first["id"].as_str().expect("an id"))
            .is_err());
        // a person's own rows across Books are countable, which is how "my Books" is answered
        assert_eq!(
            rows.everywhere("author_user_id", &json!("user-1"))
                .expect("rows")
                .len(),
            2
        );
        assert_eq!(
            rows.where_eq("book-1", "author_user_id", &json!("user-2"))
                .expect("rows")
                .len(),
            1
        );
    }

    #[test]
    fn a_row_may_not_claim_to_be_a_persons_own() {
        let graph = MemoryGraph::new();
        let rows = SharedRecords::new(&graph, ROW);
        let refused = rows.create("book-1", &json!({"user_id": "user-1", "body": "mine"}));
        assert!(refused.is_err());
        // and a change that tries the same scattergun is refused too
        let held = rows
            .create("book-1", &json!({"body": "one"}))
            .expect("a row");
        assert!(rows
            .update(
                "book-1",
                held["id"].as_str().expect("an id"),
                &json!({"user_id": "user-1"})
            )
            .is_err());
    }
}
