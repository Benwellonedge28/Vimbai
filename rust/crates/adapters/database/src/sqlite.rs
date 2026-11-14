#![forbid(unsafe_code)]
//! The on-device store: SQLite, behind the same semantic doors as every other backend.
//!
//! A laptop or a phone has one user, no server and no DBA. Before this, the device kept its records by rewriting a JSON
//! file: correct, readable, and a whole-file replace on every write, which means the whole Book is in memory and a crash
//! mid-write is survived only by the file rename underneath it. SQLite answers that with a real transaction, an index that
//! answers "this Book's entries" without reading the Book, and a lock that a second writer respects.
//!
//! What it does NOT change is the shape above it. This is `Graph`: it answers the semantic doors and refuses `run`, so
//! every caller in the engine reaches it exactly the way it reaches Neo4j. That refusal is the point of the type - if a
//! caller could hand this store a statement, the semantic trait would be decoration.
//!
//! Exact money survives because a record is stored as the JSON it arrived as, and an amount travels as a string. Nothing
//! here parses a decimal, so nothing here can round one.

use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::columns::{checked_property, id_of, indexed, now_seq, unquote};
use crate::*;
use crate::{Graph, RecordKind, StoreError};

/// One forward step of the schema, with the step that undoes it.
///
/// A migration that cannot be rolled back is a migration nobody dares run. Both halves live here so a test can apply, undo
/// and re-apply them against a store that already holds records - which is the only way to know that the third step works.
pub struct Step {
    pub name: &'static str,
    pub up: &'static str,
    pub down: &'static str,
}

/// The schema, in order. Append; never edit a step that has shipped.
pub const SCHEMA: &[Step] = &[
    Step {
        name: "0001_records",
        up: "CREATE TABLE IF NOT EXISTS records (
                 kind       TEXT NOT NULL,
                 id         TEXT NOT NULL,
                 book_id    TEXT,
                 user_id    TEXT,
                 created_at TEXT,
                 seq        INTEGER NOT NULL DEFAULT 0,
                 props      TEXT NOT NULL,
                 PRIMARY KEY (kind, id)
             );
             CREATE INDEX IF NOT EXISTS records_book ON records (book_id, kind, created_at DESC);
             CREATE INDEX IF NOT EXISTS records_user ON records (user_id, kind);",
        down: "DROP INDEX IF EXISTS records_user;
               DROP INDEX IF EXISTS records_book;
               DROP TABLE IF EXISTS records;",
    },
    Step {
        name: "0002_edges",
        up: "CREATE TABLE IF NOT EXISTS edges (
                 from_id TEXT NOT NULL,
                 label   TEXT NOT NULL,
                 to_id   TEXT NOT NULL,
                 seq     INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY (from_id, label, to_id)
             );
             CREATE INDEX IF NOT EXISTS edges_to ON edges (to_id, label);",
        down: "DROP INDEX IF EXISTS edges_to;
               DROP TABLE IF EXISTS edges;",
    },
];

/// A device's store in one file.
///
/// Doors are `&self`, so the connection lives behind a mutex: SQLite is a single-writer database by design and pretending
/// otherwise is how a device gets corruption. The mutex makes the writers take turns at the door instead of inside SQLite,
/// and WAL mode lets the readers keep reading while one does.
pub struct SqliteGraph {
    held: Mutex<Connection>,
}

impl SqliteGraph {
    /// Open (or create) a store file and bring its schema up to date.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, StoreError> {
        let connection = Connection::open(path).map_err(graph)?;
        // WAL: a reader is not blocked by a writer, which is what makes the device usable while the worker posts.
        let _: String = connection
            .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
            .map_err(graph)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(graph)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(graph)?;
        let store = Self {
            held: Mutex::new(connection),
        };
        store.migrate()?;
        Ok(store)
    }

    /// A store in memory, for tests and for the embedded profile.
    pub fn in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory().map_err(graph)?;
        let store = Self {
            held: Mutex::new(connection),
        };
        store.migrate()?;
        Ok(store)
    }

    /// The schema version the file reports.
    pub fn version(&self) -> Result<i64, StoreError> {
        let connection = self.lock()?;
        version_of(&connection)
    }

    /// Apply every step the file has not had.
    fn migrate(&self) -> Result<(), StoreError> {
        let mut connection = self.lock()?;
        apply(&mut connection)
    }

    /// Undo the last step. Exists for tests and for a release that has to go back.
    pub fn rollback(&self) -> Result<(), StoreError> {
        let mut connection = self.lock()?;
        rollback(&mut connection)
    }

    /// Apply the schema again, after a rollback. The pair is what the migration test exercises.
    pub fn reapply(&self) -> Result<(), StoreError> {
        self.migrate()
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StoreError> {
        self.held.lock().map_err(|_| {
            StoreError::Graph("the device store was left locked by a failed write".into())
        })
    }
}

fn graph(error: rusqlite::Error) -> StoreError {
    StoreError::Graph(format!("the device store: {error}"))
}

fn version_of(connection: &Connection) -> Result<i64, StoreError> {
    let present: Option<i64> = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .map_err(graph)?;
    if present.unwrap_or(0) == 0 {
        return Ok(0);
    }
    let version: Option<i64> = connection
        .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
        .optional()
        .map_err(graph)?;
    Ok(version.unwrap_or(0))
}

/// The version table is machinery rather than a step: a migration that had to create the thing that records migrations
/// would have to be recorded before it could be recorded.
fn ensure_version_table(connection: &Connection) -> Result<(), StoreError> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                 version INTEGER NOT NULL
             );",
        )
        .map_err(graph)
}

fn apply(connection: &mut Connection) -> Result<(), StoreError> {
    ensure_version_table(connection)?;
    let mut version = version_of(connection)? as usize;
    if version > SCHEMA.len() {
        return Err(StoreError::Graph(format!(
            "this store was written by a newer build: schema {version}, this build knows {}",
            SCHEMA.len()
        )));
    }
    while version < SCHEMA.len() {
        let step = &SCHEMA[version];
        // one transaction per step: a step that fails half way leaves the previous version, not a half-schema
        let transaction = connection.transaction().map_err(graph)?;
        transaction.execute_batch(step.up).map_err(graph)?;
        transaction
            .execute_batch("DELETE FROM schema_version;")
            .map_err(graph)?;
        transaction
            .execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                params![(version + 1) as i64],
            )
            .map_err(graph)?;
        transaction.commit().map_err(graph)?;
        version += 1;
    }
    Ok(())
}

fn rollback(connection: &mut Connection) -> Result<(), StoreError> {
    let version = version_of(connection)? as usize;
    if version == 0 {
        return Ok(());
    }
    let step = &SCHEMA[version - 1];
    let transaction = connection.transaction().map_err(graph)?;
    transaction.execute_batch(step.down).map_err(graph)?;
    transaction.commit().map_err(graph)?;
    // the version is one row, so it is replaced rather than appended: a second row would make "which version is this"
    // depend on row order
    connection
        .execute("DELETE FROM schema_version;", [])
        .map_err(graph)?;
    if version > 1 {
        connection
            .execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                params![(version - 1) as i64],
            )
            .map_err(graph)?;
    }
    Ok(())
}

impl Graph for SqliteGraph {
    /// Refused on purpose.
    ///
    /// This store answers the semantic doors and nothing else. Keeping the refusal here means the trait's escape hatch
    /// cannot quietly become this backend's interface: a caller that wants a statement answered would fail loudly here
    /// instead of becoming a Cypher-shaped dependency of a database that has none.
    fn run(&self, _cypher: &str, _params: Value) -> Result<Vec<Value>, StoreError> {
        Err(StoreError::Graph(
            "the device store answers operations, not statements".into(),
        ))
    }

    fn store_create(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        props: Value,
    ) -> Result<Value, StoreError> {
        let connection = self.lock()?;
        insert_one(&connection, kind.label, &props)?;
        insert_edge(&connection, &scope.user_id, kind.edge, id_of(&props)?)?;
        Ok(props)
    }

    fn store_create_with_children(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        props: Value,
        child: RecordKind,
        children: Vec<Value>,
    ) -> Result<Value, StoreError> {
        let mut connection = self.lock()?;
        // ONE transaction: an entry without its lines, or lines that are not postings, is a state a ledger may not exist in
        let transaction = connection.transaction().map_err(graph)?;
        let parent = id_of(&props)?.to_owned();
        insert_one(&transaction, kind.label, &props)?;
        insert_edge(&transaction, &scope.user_id, kind.edge, &parent)?;
        for line in &children {
            insert_one(&transaction, child.label, line)?;
            insert_edge(&transaction, &parent, child.edge, id_of(line)?)?;
            if let (Some(account), Some(side)) = (
                line.get("account_number").and_then(Value::as_str),
                line.get("side").and_then(Value::as_str),
            ) {
                if let Some(account_id) = account_id(&transaction, &scope.book_id, account)? {
                    let label = if side == "debit" {
                        "DEBITED"
                    } else {
                        "CREDITED"
                    };
                    insert_edge(&transaction, id_of(line)?, label, &account_id)?;
                }
            }
        }
        transaction.commit().map_err(graph)?;
        let mut answer = props;
        answer["lines"] = Value::Array(children);
        Ok(answer)
    }

    fn store_get(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        id: &str,
    ) -> Result<Value, StoreError> {
        let connection = self.lock()?;
        owned(&connection, kind.label, &scope.book_id, id)?.ok_or(StoreError::NotFound)
    }

    fn store_get_with_children(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        id: &str,
        child: RecordKind,
    ) -> Result<Value, StoreError> {
        let connection = self.lock()?;
        let mut record =
            owned(&connection, kind.label, &scope.book_id, id)?.ok_or(StoreError::NotFound)?;
        record["lines"] = Value::Array(children_of(&connection, child, id)?);
        Ok(record)
    }

    fn store_list(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        limit: usize,
    ) -> Result<Vec<Value>, StoreError> {
        let connection = self.lock()?;
        listed(&connection, kind.label, Some(&scope.book_id), limit)
    }

    fn store_list_with_children(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        limit: usize,
        child: RecordKind,
    ) -> Result<Vec<Value>, StoreError> {
        let connection = self.lock()?;
        let mut records = listed(&connection, kind.label, Some(&scope.book_id), limit)?;
        for record in &mut records {
            let parent = record
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            record["lines"] = Value::Array(children_of(&connection, child, &parent)?);
        }
        Ok(records)
    }

    fn store_update(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        id: &str,
        patch: Value,
    ) -> Result<Value, StoreError> {
        let connection = self.lock()?;
        let mut record =
            owned(&connection, kind.label, &scope.book_id, id)?.ok_or(StoreError::NotFound)?;
        if let (Some(held), Some(given)) = (record.as_object_mut(), patch.as_object()) {
            for (name, value) in given {
                held.insert(name.clone(), value.clone());
            }
        }
        // an update may not move a record between Books, so the scope columns are written from the stored record
        let (book, user, created) = indexed(&record);
        connection
            .execute(
                "UPDATE records SET props = ?1, book_id = ?2, user_id = ?3, created_at = ?4 WHERE kind = ?5 AND id = ?6",
                params![record.to_string(), book, user, created, kind.label, id],
            )
            .map_err(graph)?;
        Ok(record)
    }

    fn store_delete(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        id: &str,
    ) -> Result<(), StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction().map_err(graph)?;
        let removed = transaction
            .execute(
                "DELETE FROM records WHERE kind = ?1 AND id = ?2 AND book_id = ?3",
                params![kind.label, id, scope.book_id],
            )
            .map_err(graph)?;
        if removed == 0 {
            return Err(StoreError::NotFound);
        }
        transaction
            .execute(
                "DELETE FROM edges WHERE from_id = ?1 OR to_id = ?1",
                params![id],
            )
            .map_err(graph)?;
        transaction.commit().map_err(graph)?;
        Ok(())
    }

    fn store_find(
        &self,
        kind: RecordKind,
        property: &str,
        token: &str,
    ) -> Result<Option<Value>, StoreError> {
        let connection = self.lock()?;
        find_by(&connection, kind.label, property, token, None)
    }

    fn store_lines_for_account(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        child: RecordKind,
        account_number: &str,
        limit: usize,
    ) -> Result<Vec<Value>, StoreError> {
        let connection = self.lock()?;
        let account =
            account_id(&connection, &scope.book_id, account_number)?.ok_or(StoreError::NotFound)?;
        // walked from the account inwards, so the answer does not depend on how many entries the Book holds
        let mut statement = connection
            .prepare(
                "SELECT l.props, p.props
                 FROM edges e
                 JOIN records l ON l.id = e.from_id AND l.kind = ?1
                 JOIN edges own ON own.to_id = l.id AND own.label = ?2
                 JOIN records p ON p.id = own.from_id AND p.kind = ?3
                 WHERE e.to_id = ?4 AND e.label IN ('DEBITED', 'CREDITED') AND l.book_id = ?5
                 ORDER BY json_extract(p.props, '$.entry_date') DESC, l.seq DESC
                 LIMIT ?6",
            )
            .map_err(graph)?;
        let rows = statement
            .query_map(
                params![
                    child.label,
                    child.edge,
                    kind.label,
                    account,
                    scope.book_id,
                    limit as i64
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(graph)?;
        let mut answer = Vec::new();
        for row in rows {
            let (line, entry) = row.map_err(graph)?;
            let line = unquote(&line)?;
            let entry = unquote(&entry)?;
            answer.push(json!({
                "l": line,
                "entry_id": entry.get("id").cloned().unwrap_or(Value::Null),
                "entry_date": entry.get("entry_date").cloned().unwrap_or(Value::Null),
                "entry_description": entry.get("description").cloned().unwrap_or(Value::Null),
            }));
        }
        Ok(answer)
    }

    fn all_edges(&self) -> Result<Vec<(String, String, String)>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT from_id, label, to_id FROM edges ORDER BY seq, from_id, label, to_id")
            .map_err(graph)?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(graph)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(graph)
    }

    fn create_edge(&self, from: &str, label: &str, to: &str) -> Result<(), StoreError> {
        let connection = self.lock()?;
        insert_edge(&connection, from, label, to)
    }

    fn store_create_platform(&self, kind: RecordKind, props: Value) -> Result<Value, StoreError> {
        let connection = self.lock()?;
        // a platform write REPLACES a record with the same id rather than colliding with it. That is not laxity: this door
        // is what a restore and a resumed sync both go through, and a migration that has to be safe to re-run cannot fail
        // on the second attempt because the first one got half way.
        insert_replacing(&connection, kind.label, &props)?;
        Ok(props)
    }

    fn store_get_platform(&self, kind: RecordKind, id: &str) -> Result<Option<Value>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT props FROM records WHERE kind = ?1 AND id = ?2")
            .map_err(graph)?;
        let text: Option<String> = statement
            .query_row(params![kind.label, id], |row| row.get(0))
            .optional()
            .map_err(graph)?;
        text.map(|text| unquote(&text)).transpose()
    }

    fn store_find_platform(
        &self,
        kind: RecordKind,
        property: &str,
        value: &str,
    ) -> Result<Option<Value>, StoreError> {
        let connection = self.lock()?;
        find_by(&connection, kind.label, property, value, None)
    }

    fn store_matching_platform(
        &self,
        kind: RecordKind,
        property: &str,
        value: &str,
        limit: usize,
    ) -> Result<Vec<Value>, StoreError> {
        let connection = self.lock()?;
        matching_by(&connection, kind.label, property, value, None, limit)
    }

    fn store_list_platform(
        &self,
        kind: RecordKind,
        limit: usize,
    ) -> Result<Vec<Value>, StoreError> {
        let connection = self.lock()?;
        listed(&connection, kind.label, None, limit)
    }

    fn store_remove_platform(&self, kind: RecordKind, id: &str) -> Result<bool, StoreError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction().map_err(graph)?;
        let removed = transaction
            .execute(
                "DELETE FROM records WHERE kind = ?1 AND id = ?2",
                params![kind.label, id],
            )
            .map_err(graph)?;
        transaction
            .execute(
                "DELETE FROM edges WHERE from_id = ?1 OR to_id = ?1",
                params![id],
            )
            .map_err(graph)?;
        transaction.commit().map_err(graph)?;
        Ok(removed > 0)
    }

    fn store_update_platform(
        &self,
        kind: RecordKind,
        id: &str,
        patch: Value,
    ) -> Result<Option<Value>, StoreError> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT props FROM records WHERE kind = ?1 AND id = ?2")
            .map_err(graph)?;
        let text: Option<String> = statement
            .query_row(params![kind.label, id], |row| row.get(0))
            .optional()
            .map_err(graph)?;
        let Some(text) = text else {
            return Ok(None);
        };
        let mut record = unquote(&text)?;
        if let (Some(held), Some(given)) = (record.as_object_mut(), patch.as_object()) {
            for (name, value) in given {
                held.insert(name.clone(), value.clone());
            }
        }
        let (book, user, created) = indexed(&record);
        connection
            .execute(
                "UPDATE records SET props = ?1, book_id = ?2, user_id = ?3, created_at = ?4 WHERE kind = ?5 AND id = ?6",
                params![record.to_string(), book, user, created, kind.label, id],
            )
            .map_err(graph)?;
        Ok(Some(record))
    }

    fn store_shared_create(
        &self,
        kind: RecordKind,
        book_id: &str,
        props: Value,
    ) -> Result<Value, StoreError> {
        let mut props = props;
        props["book_id"] = json!(book_id);
        self.shared_create(kind, &props)
    }

    fn store_shared_get(
        &self,
        kind: RecordKind,
        book_id: &str,
        id: &str,
    ) -> Result<Value, StoreError> {
        self.shared_get(kind, book_id, id)
    }

    fn store_shared_list(
        &self,
        kind: RecordKind,
        book_id: &str,
        limit: usize,
    ) -> Result<Vec<Value>, StoreError> {
        self.shared_list(kind, book_id, limit)
    }

    fn store_shared_where(
        &self,
        kind: RecordKind,
        book_id: &str,
        property: &str,
        value: Value,
    ) -> Result<Vec<Value>, StoreError> {
        self.shared_where(kind, book_id, property, &value)
    }

    fn store_shared_everywhere(
        &self,
        kind: RecordKind,
        property: &str,
        value: Value,
    ) -> Result<Vec<Value>, StoreError> {
        self.shared_everywhere(kind, property, &value)
    }

    fn store_shared_update(
        &self,
        kind: RecordKind,
        book_id: &str,
        id: &str,
        patch: Value,
    ) -> Result<Value, StoreError> {
        self.shared_update(kind, book_id, id, patch)
    }

    fn store_shared_delete(
        &self,
        kind: RecordKind,
        book_id: &str,
        id: &str,
    ) -> Result<(), StoreError> {
        self.shared_delete(kind, book_id, id)
    }
}

impl SqliteGraph {
    /// The shared-row doors, opened separately so the trait's defaults are never what this backend answers with.
    ///
    /// They are the same operations as the owned ones minus the owner edge, and a backend that left them to the default
    /// would be answering those callers with a statement - which this store refuses. Keeping them here means the round trip
    /// through `SharedRecords` is native too.
    fn shared_create(&self, kind: RecordKind, props: &Value) -> Result<Value, StoreError> {
        let connection = self.lock()?;
        insert_one(&connection, kind.label, props)?;
        Ok(props.clone())
    }

    fn shared_get(&self, kind: RecordKind, book_id: &str, id: &str) -> Result<Value, StoreError> {
        let connection = self.lock()?;
        owned(&connection, kind.label, book_id, id)?.ok_or(StoreError::NotFound)
    }

    fn shared_list(
        &self,
        kind: RecordKind,
        book_id: &str,
        limit: usize,
    ) -> Result<Vec<Value>, StoreError> {
        let connection = self.lock()?;
        listed(&connection, kind.label, Some(book_id), limit)
    }

    fn shared_where(
        &self,
        kind: RecordKind,
        book_id: &str,
        property: &str,
        value: &Value,
    ) -> Result<Vec<Value>, StoreError> {
        let token = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        let connection = self.lock()?;
        matching_by(
            &connection,
            kind.label,
            property,
            &token,
            Some(book_id),
            i64::MAX as usize,
        )
    }

    fn shared_everywhere(
        &self,
        kind: RecordKind,
        property: &str,
        value: &Value,
    ) -> Result<Vec<Value>, StoreError> {
        let token = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        let connection = self.lock()?;
        matching_by(
            &connection,
            kind.label,
            property,
            &token,
            None,
            i64::MAX as usize,
        )
    }

    fn shared_update(
        &self,
        kind: RecordKind,
        book_id: &str,
        id: &str,
        patch: Value,
    ) -> Result<Value, StoreError> {
        let connection = self.lock()?;
        let mut record =
            owned(&connection, kind.label, book_id, id)?.ok_or(StoreError::NotFound)?;
        if let (Some(held), Some(given)) = (record.as_object_mut(), patch.as_object()) {
            for (name, value) in given {
                held.insert(name.clone(), value.clone());
            }
        }
        let (book, user, created) = indexed(&record);
        connection
            .execute(
                "UPDATE records SET props = ?1, book_id = ?2, user_id = ?3, created_at = ?4 WHERE kind = ?5 AND id = ?6",
                params![record.to_string(), book, user, created, kind.label, id],
            )
            .map_err(graph)?;
        Ok(record)
    }

    fn shared_delete(&self, kind: RecordKind, book_id: &str, id: &str) -> Result<(), StoreError> {
        remove_record(self, kind.label, Some(book_id), id)
    }
}

fn remove_record(
    store: &SqliteGraph,
    kind: &str,
    book_id: Option<&str>,
    id: &str,
) -> Result<(), StoreError> {
    let mut connection = store.lock()?;
    let transaction = connection.transaction().map_err(graph)?;
    let removed = transaction
        .execute(
            "DELETE FROM records WHERE kind = ?1 AND id = ?2 AND (?3 IS NULL OR book_id = ?3)",
            params![kind, id, book_id],
        )
        .map_err(graph)?;
    if removed == 0 {
        return Err(StoreError::NotFound);
    }
    transaction
        .execute(
            "DELETE FROM edges WHERE from_id = ?1 OR to_id = ?1",
            params![id],
        )
        .map_err(graph)?;
    transaction.commit().map_err(graph)?;
    Ok(())
}

fn insert_one(connection: &Connection, kind: &str, props: &Value) -> Result<(), StoreError> {
    let (book, user, created) = indexed(props);
    connection
        .execute(
            "INSERT INTO records (kind, id, book_id, user_id, created_at, seq, props)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                kind,
                id_of(props)?,
                book,
                user,
                created,
                // insertion order, so two records stamped in the same second still have a stable order
                now_seq(),
                props.to_string()
            ],
        )
        .map_err(graph)?;
    Ok(())
}

fn insert_replacing(connection: &Connection, kind: &str, props: &Value) -> Result<(), StoreError> {
    let (book, user, created) = indexed(props);
    connection
        .execute(
            "INSERT INTO records (kind, id, book_id, user_id, created_at, seq, props)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (kind, id) DO UPDATE SET
                 book_id = excluded.book_id,
                 user_id = excluded.user_id,
                 created_at = excluded.created_at,
                 props = excluded.props",
            params![
                kind,
                id_of(props)?,
                book,
                user,
                created,
                now_seq(),
                props.to_string()
            ],
        )
        .map_err(graph)?;
    Ok(())
}

fn insert_edge(
    connection: &Connection,
    from: &str,
    label: &str,
    to: &str,
) -> Result<(), StoreError> {
    connection
        .execute(
            "INSERT INTO edges (from_id, label, to_id, seq) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (from_id, label, to_id) DO NOTHING",
            params![from, label, to, now_seq()],
        )
        .map_err(graph)?;
    Ok(())
}

fn owned(
    connection: &Connection,
    kind: &str,
    book_id: &str,
    id: &str,
) -> Result<Option<Value>, StoreError> {
    let mut statement = connection
        .prepare("SELECT props FROM records WHERE kind = ?1 AND id = ?2 AND book_id = ?3")
        .map_err(graph)?;
    let text: Option<String> = statement
        .query_row(params![kind, id, book_id], |row| row.get(0))
        .optional()
        .map_err(graph)?;
    text.map(|text| unquote(&text)).transpose()
}

fn listed(
    connection: &Connection,
    kind: &str,
    book_id: Option<&str>,
    limit: usize,
) -> Result<Vec<Value>, StoreError> {
    let mut statement = connection
        .prepare(
            "SELECT props FROM records
             WHERE kind = ?1 AND (?2 IS NULL OR book_id = ?2)
             ORDER BY created_at DESC, seq DESC
             LIMIT ?3",
        )
        .map_err(graph)?;
    let rows = statement
        .query_map(params![kind, book_id, limit as i64], |row| {
            row.get::<_, String>(0)
        })
        .map_err(graph)?;
    let mut answer = Vec::new();
    for row in rows {
        answer.push(unquote(&row.map_err(graph)?)?);
    }
    Ok(answer)
}

fn children_of(
    connection: &Connection,
    child: RecordKind,
    parent: &str,
) -> Result<Vec<Value>, StoreError> {
    let mut statement = connection
        .prepare(
            "SELECT r.props FROM edges e JOIN records r ON r.id = e.to_id AND r.kind = ?1
             WHERE e.from_id = ?2 AND e.label = ?3
             ORDER BY json_extract(r.props, '$.line_no'), r.seq",
        )
        .map_err(graph)?;
    let rows = statement
        .query_map(params![child.label, parent, child.edge], |row| {
            row.get::<_, String>(0)
        })
        .map_err(graph)?;
    let mut answer = Vec::new();
    for row in rows {
        answer.push(unquote(&row.map_err(graph)?)?);
    }
    Ok(answer)
}

fn find_by(
    connection: &Connection,
    kind: &str,
    property: &str,
    value: &str,
    book_id: Option<&str>,
) -> Result<Option<Value>, StoreError> {
    let property = checked_property(property)?;
    let path = format!("$.{property}");
    let mut statement = connection
        .prepare(
            "SELECT props FROM records
             WHERE kind = ?1 AND json_extract(props, ?2) = ?3 AND (?4 IS NULL OR book_id = ?4)
             ORDER BY seq DESC LIMIT 1",
        )
        .map_err(graph)?;
    let text: Option<String> = statement
        .query_row(params![kind, path, value, book_id], |row| row.get(0))
        .optional()
        .map_err(graph)?;
    text.map(|text| unquote(&text)).transpose()
}

fn matching_by(
    connection: &Connection,
    kind: &str,
    property: &str,
    value: &str,
    book_id: Option<&str>,
    limit: usize,
) -> Result<Vec<Value>, StoreError> {
    let property = checked_property(property)?;
    let path = format!("$.{property}");
    let mut statement = connection
        .prepare(
            "SELECT props FROM records
             WHERE kind = ?1 AND json_extract(props, ?2) = ?3 AND (?4 IS NULL OR book_id = ?4)
             ORDER BY created_at DESC, seq DESC LIMIT ?5",
        )
        .map_err(graph)?;
    let rows = statement
        .query_map(params![kind, path, value, book_id, limit as i64], |row| {
            row.get::<_, String>(0)
        })
        .map_err(graph)?;
    let mut answer = Vec::new();
    for row in rows {
        answer.push(unquote(&row.map_err(graph)?)?);
    }
    Ok(answer)
}

fn account_id(
    connection: &Connection,
    book_id: &str,
    account_number: &str,
) -> Result<Option<String>, StoreError> {
    let mut statement = connection
        .prepare(
            "SELECT id FROM records
             WHERE kind = 'Account' AND book_id = ?1 AND json_extract(props, '$.account_number') = ?2
             LIMIT 1",
        )
        .map_err(graph)?;
    statement
        .query_row(params![book_id, account_number], |row| row.get(0))
        .optional()
        .map_err(graph)
}

/// A record's properties as a map, for the tests and for a caller that wants to read one without a door for it.
pub fn properties(record: &Value) -> Map<String, Value> {
    record.as_object().cloned().unwrap_or_default()
}
