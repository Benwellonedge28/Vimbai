//! The deployment store: Postgres, behind the same semantic doors as the device store.
//!
//! What this backend buys over the JSON file a deployment used to overflow into is not speed:
//!
//! - one transaction for an entry and its lines, and for the edges that say which account each line touched, so a crash
//!   leaves a whole posting or none of it;
//! - a row lock per Book instead of a whole-store lock, so two people in different Books never wait for each other;
//! - `numeric` for an amount if a caller ever wants the database to add two of them, which is why amounts are stored as
//!   their exact text rather than as a float;
//! - an index that answers "this account's postings" by walking back from the account, not by reading the Book.
//!
//! The doors are the only interface. `run` is refused here for the same reason it is refused on the device store: a
//! backend that answered statements would make every future backend a query interpreter, and the whole point of the
//! semantic trait was to stop that.
//!
//! ## Connections
//!
//! A small pool of clients rather than one guarded client: a Book's write and another Book's read are not in each other's
//! way, and a pool that is exhausted blocks rather than failing, because "the database is busy" is not an answer a ledger
//! may give to a person saving an entry.

use std::sync::{Mutex, MutexGuard};

use postgres::{Client, GenericClient, NoTls, Row};
use serde_json::{json, Value};

use crate::columns::{checked_property, id_of};
use crate::{
    BookScope, Graph, RecordKind, StoreError, JOURNAL_ENTRY, JOURNAL_LINE, LEDGER_ACCOUNT,
};

/// One record per row: the kind and id identify it, the columns beside them are the ones a query needs to filter on
/// without opening the JSON, and `props` is the record itself, kept exactly as the caller wrote it.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS store_records (
    kind        text  NOT NULL,
    id          text  NOT NULL,
    book_id     text,
    user_id     text,
    created_at  text,
    seq         bigint NOT NULL,
    props       jsonb NOT NULL,
    PRIMARY KEY (kind, id)
);
CREATE INDEX IF NOT EXISTS store_records_by_book ON store_records (kind, book_id, created_at DESC, seq DESC);
CREATE INDEX IF NOT EXISTS store_records_by_seq  ON store_records (seq);
CREATE TABLE IF NOT EXISTS store_edges (
    from_id text NOT NULL,
    label   text NOT NULL,
    to_id   text NOT NULL,
    seq     bigint NOT NULL,
    PRIMARY KEY (from_id, label, to_id)
);
CREATE INDEX IF NOT EXISTS store_edges_to ON store_edges (to_id, label);
";

/// A store in a deployment, holding a pool of connections into one database.
pub struct PostgresGraph {
    dsn: String,
    pool: Mutex<Vec<Client>>,
    /// How many connections this store will hold open at once.
    capacity: usize,
}

impl PostgresGraph {
    /// Open a store, creating the tables the first time.
    pub fn connect(dsn: &str, capacity: usize) -> Result<Self, StoreError> {
        let store = Self {
            dsn: dsn.to_owned(),
            pool: Mutex::new(Vec::new()),
            capacity: capacity.max(1),
        };
        store.with(|client| client.batch_execute(SCHEMA).map_err(fault))?;
        Ok(store)
    }

    /// A store pointed at a database, creating the schema, at the address `VIMBAI_POSTGRES_URL` names.
    pub fn from_env() -> Result<Self, StoreError> {
        let dsn = std::env::var("VIMBAI_POSTGRES_URL").map_err(|_| {
            StoreError::Invalid(
                "VIMBAI_POSTGRES_URL is not set, so there is no deployment store to open".into(),
            )
        })?;
        let capacity = std::env::var("VIMBAI_POSTGRES_POOL")
            .ok()
            .and_then(|text| text.parse().ok())
            .unwrap_or(8);
        Self::connect(&dsn, capacity)
    }

    fn connection(&self) -> Result<Client, StoreError> {
        {
            let mut pool = self.held()?;
            if let Some(client) = pool.pop() {
                // a connection that died while it sat in the pool (a restart, a killed backend) is replaced rather than
                // handed out: the caller wanted a connection, not an error from somebody else's crash
                if !client.is_closed() {
                    return Ok(client);
                }
            }
        }
        Client::connect(&self.dsn, NoTls).map_err(fault)
    }

    /// Borrow a connection for the length of one operation, and put it back.
    ///
    /// The work runs on a thread of its own, always, and that is not incidental: this client is synchronous and drives its
    /// own runtime internally, so a query issued from inside the engine's async runtime panics with "cannot start a runtime
    /// from within a runtime" - which is exactly what happened the first time the engine was pointed at Postgres. Doing the
    /// blocking work off the async threads is also what keeps a slow query from stalling other requests.
    fn with<T: Send>(
        &self,
        operation: impl FnOnce(&mut Client) -> Result<T, StoreError> + Send,
    ) -> Result<T, StoreError> {
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let mut client = self.connection()?;
                let answer = operation(&mut client);
                // only a healthy connection goes back: one that failed mid-statement may be inside a broken transaction
                if answer.is_ok() && !client.is_closed() {
                    if let Ok(mut pool) = self.held() {
                        if pool.len() < self.capacity {
                            pool.push(client);
                        }
                    }
                }
                answer
            });
            worker.join().unwrap_or_else(|_| {
                Err(StoreError::Graph(
                    "the deployment store's worker thread panicked, so the answer is unknown"
                        .into(),
                ))
            })
        })
    }

    fn held(&self) -> Result<MutexGuard<'_, Vec<Client>>, StoreError> {
        self.pool
            .lock()
            .map_err(|_| StoreError::Graph("the connection pool was poisoned".into()))
    }
}

fn fault(error: postgres::Error) -> StoreError {
    let mut detail = format!("{error}");
    let mut source: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(&error);
    while let Some(cause) = source {
        detail.push_str(&format!(" <- {cause}"));
        source = cause.source();
    }
    StoreError::Graph(format!("the deployment store: {detail}"))
}

fn rows_to_value(rows: &[Row], column: &str) -> Result<Vec<Value>, StoreError> {
    rows.iter()
        .map(|row| row.try_get::<_, Value>(column).map_err(fault))
        .collect()
}

fn insert(
    client: &mut impl GenericClient,
    kind: &str,
    props: &Value,
    replacing: bool,
) -> Result<(), StoreError> {
    let book = props.get("book_id").and_then(Value::as_str);
    let user = props.get("user_id").and_then(Value::as_str);
    let created = props.get("created_at").and_then(Value::as_str);
    let record = props.clone();
    let statement = if replacing {
        // a write through the platform door replaces a record with the same id: a restore and a resumed sync both come
        // through here, and a migration that must be safe to re-run cannot fail on its second attempt
        "INSERT INTO store_records (kind, id, book_id, user_id, created_at, seq, props)
         VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb)
         ON CONFLICT (kind, id) DO UPDATE SET
             book_id = excluded.book_id,
             user_id = excluded.user_id,
             created_at = excluded.created_at,
             props = excluded.props"
    } else {
        "INSERT INTO store_records (kind, id, book_id, user_id, created_at, seq, props)
         VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb)"
    };
    client
        .execute(
            statement,
            &[
                &kind,
                &id_of(props)?,
                &book,
                &user,
                &created,
                &version(),
                &record,
            ],
        )
        .map_err(fault)?;
    Ok(())
}

fn link(
    client: &mut impl GenericClient,
    from: &str,
    label: &str,
    to: &str,
) -> Result<(), StoreError> {
    client
        .execute(
            "INSERT INTO store_edges (from_id, label, to_id, seq) VALUES ($1, $2, $3, $4)
             ON CONFLICT (from_id, label, to_id) DO NOTHING",
            &[&from, &label, &to, &version()],
        )
        .map_err(fault)?;
    Ok(())
}

/// A store-wide increasing number, so "newest first" is stable even inside one microsecond.
fn version() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_micros() as i64)
        .unwrap_or(0)
}

impl Graph for PostgresGraph {
    /// Refused on purpose, for the same reason the device store refuses it.
    fn run(&self, _cypher: &str, _params: Value) -> Result<Vec<Value>, StoreError> {
        Err(StoreError::Graph(
            "the deployment store answers operations, not statements".into(),
        ))
    }

    fn store_create(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        props: Value,
    ) -> Result<Value, StoreError> {
        self.with(|client| {
            let mut transaction = client.transaction().map_err(fault)?;
            insert(&mut transaction, kind.label, &props, false)?;
            link(&mut transaction, &scope.user_id, kind.edge, id_of(&props)?)?;
            transaction.commit().map_err(fault)?;
            Ok(props)
        })
    }

    fn store_create_with_children(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        props: Value,
        child: RecordKind,
        children: Vec<Value>,
    ) -> Result<Value, StoreError> {
        self.with(|client| {
            // one transaction: an entry without its lines, or lines that are not postings, is a state a ledger may not
            // exist in
            let mut transaction = client.transaction().map_err(fault)?;
            let parent = id_of(&props)?.to_owned();
            insert(&mut transaction, kind.label, &props, false)?;
            link(&mut transaction, &scope.user_id, kind.edge, &parent)?;
            for line in &children {
                insert(&mut transaction, child.label, line, false)?;
                link(&mut transaction, &parent, child.edge, id_of(line)?)?;
                if let (Some(account), Some(side)) = (
                    line.get("account_number").and_then(Value::as_str),
                    line.get("side").and_then(Value::as_str),
                ) {
                    if let Some(account_id) = account_id(&mut transaction, &scope.book_id, account)?
                    {
                        let label = if side == "debit" {
                            "DEBITED"
                        } else {
                            "CREDITED"
                        };
                        link(&mut transaction, id_of(line)?, label, &account_id)?;
                    }
                }
            }
            transaction.commit().map_err(fault)?;
            let mut answer = props;
            answer["lines"] = Value::Array(children);
            Ok(answer)
        })
    }

    fn store_get(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        id: &str,
    ) -> Result<Value, StoreError> {
        self.with(|client| {
            owned(client, kind.label, &scope.book_id, id)?.ok_or(StoreError::NotFound)
        })
    }

    fn store_get_with_children(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        id: &str,
        child: RecordKind,
    ) -> Result<Value, StoreError> {
        self.with(|client| {
            let mut record =
                owned(client, kind.label, &scope.book_id, id)?.ok_or(StoreError::NotFound)?;
            record["lines"] = Value::Array(children_of(client, child, id)?);
            Ok(record)
        })
    }

    fn store_list(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        limit: usize,
    ) -> Result<Vec<Value>, StoreError> {
        self.with(|client| listed(client, kind.label, Some(&scope.book_id), limit))
    }

    fn store_list_with_children(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        limit: usize,
        child: RecordKind,
    ) -> Result<Vec<Value>, StoreError> {
        self.with(|client| {
            let mut records = listed(client, kind.label, Some(&scope.book_id), limit)?;
            for record in &mut records {
                let parent = record
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                record["lines"] = Value::Array(children_of(client, child, &parent)?);
            }
            Ok(records)
        })
    }

    fn store_update(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        id: &str,
        patch: Value,
    ) -> Result<Value, StoreError> {
        self.with(|client| {
            let mut record =
                owned(client, kind.label, &scope.book_id, id)?.ok_or(StoreError::NotFound)?;
            if let (Some(held), Some(given)) = (record.as_object_mut(), patch.as_object()) {
                for (name, value) in given {
                    held.insert(name.clone(), value.clone());
                }
            }
            // a record may not be moved between Books by an update, so the scope columns come from the stored record
            let book = record.get("book_id").and_then(Value::as_str);
            let user = record.get("user_id").and_then(Value::as_str);
            let created = record.get("created_at").and_then(Value::as_str);
            client
                .execute(
                    "UPDATE store_records SET props = $1::jsonb, book_id = $2, user_id = $3, created_at = $4
                     WHERE kind = $5 AND id = $6",
                    &[&record, &book, &user, &created, &kind.label, &id],
                )
                .map_err(fault)?;
            Ok(record)
        })
    }

    fn store_delete(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        id: &str,
    ) -> Result<(), StoreError> {
        self.with(|client| {
            let mut transaction = client.transaction().map_err(fault)?;
            let removed = transaction
                .execute(
                    "DELETE FROM store_records WHERE kind = $1 AND id = $2 AND book_id = $3",
                    &[&kind.label, &id, &scope.book_id],
                )
                .map_err(fault)?;
            if removed == 0 {
                return Err(StoreError::NotFound);
            }
            transaction
                .execute(
                    "DELETE FROM store_edges WHERE from_id = $1 OR to_id = $1",
                    &[&id],
                )
                .map_err(fault)?;
            transaction.commit().map_err(fault)?;
            Ok(())
        })
    }

    fn store_find(
        &self,
        kind: RecordKind,
        property: &str,
        token: &str,
    ) -> Result<Option<Value>, StoreError> {
        let property = checked_property(property)?;
        self.with(|client| {
            let row = client
                .query_opt(
                    "SELECT props FROM store_records
                     WHERE kind = $1 AND props ->> $2 = $3
                     ORDER BY created_at DESC, seq DESC LIMIT 1",
                    &[&kind.label, &property, &token],
                )
                .map_err(fault)?;
            row.map(|row| row.try_get::<_, Value>("props").map_err(fault))
                .transpose()
        })
    }

    fn store_lines_for_account(
        &self,
        kind: RecordKind,
        scope: &BookScope,
        child: RecordKind,
        account_number: &str,
        limit: usize,
    ) -> Result<Vec<Value>, StoreError> {
        self.with(|client| {
            let account =
                account_id(client, &scope.book_id, account_number)?.ok_or(StoreError::NotFound)?;
            // walked from the account inwards, so the answer does not depend on how many entries the Book holds
            let rows = client
                .query(
                    "SELECT l.props AS line, p.props AS entry
                     FROM store_edges e
                     JOIN store_records l ON l.id = e.from_id AND l.kind = $1
                     JOIN store_edges own ON own.to_id = l.id AND own.label = $2
                     JOIN store_records p ON p.id = own.from_id AND p.kind = $3
                     WHERE e.to_id = $4 AND e.label IN ('DEBITED', 'CREDITED') AND l.book_id = $5
                     ORDER BY p.props ->> 'entry_date' DESC, l.seq DESC
                     LIMIT $6",
                    &[
                        &child.label,
                        &child.edge,
                        &kind.label,
                        &account,
                        &scope.book_id,
                        &(limit as i64),
                    ],
                )
                .map_err(fault)?;
            let mut answer = Vec::new();
            for row in rows {
                let line: Value = row.try_get("line").map_err(fault)?;
                let entry: Value = row.try_get("entry").map_err(fault)?;
                answer.push(json!({
                    "l": line,
                    "entry_id": entry.get("id").cloned().unwrap_or(Value::Null),
                    "entry_date": entry.get("entry_date").cloned().unwrap_or(Value::Null),
                    "entry_description": entry.get("description").cloned().unwrap_or(Value::Null),
                }));
            }
            Ok(answer)
        })
    }

    fn all_edges(&self) -> Result<Vec<(String, String, String)>, StoreError> {
        self.with(|client| {
            let rows = client
                .query(
                    "SELECT from_id, label, to_id FROM store_edges ORDER BY seq, from_id, label, to_id",
                    &[],
                )
                .map_err(fault)?;
            rows.iter()
                .map(|row| {
                    Ok((
                        row.try_get::<_, String>(0).map_err(fault)?,
                        row.try_get::<_, String>(1).map_err(fault)?,
                        row.try_get::<_, String>(2).map_err(fault)?,
                    ))
                })
                .collect()
        })
    }

    fn create_edge(&self, from: &str, label: &str, to: &str) -> Result<(), StoreError> {
        self.with(|client| link(client, from, label, to))
    }

    fn store_create_platform(&self, kind: RecordKind, props: Value) -> Result<Value, StoreError> {
        self.with(|client| {
            insert(client, kind.label, &props, true)?;
            Ok(props)
        })
    }

    fn store_get_platform(&self, kind: RecordKind, id: &str) -> Result<Option<Value>, StoreError> {
        self.with(|client| {
            let row = client
                .query_opt(
                    "SELECT props FROM store_records WHERE kind = $1 AND id = $2",
                    &[&kind.label, &id],
                )
                .map_err(fault)?;
            row.map(|row| row.try_get::<_, Value>("props").map_err(fault))
                .transpose()
        })
    }

    fn store_find_platform(
        &self,
        kind: RecordKind,
        property: &str,
        token: &str,
    ) -> Result<Option<Value>, StoreError> {
        let property = checked_property(property)?;
        self.with(|client| {
            let row = client
                .query_opt(
                    "SELECT props FROM store_records
                     WHERE kind = $1 AND props ->> $2 = $3
                     ORDER BY created_at DESC, seq DESC LIMIT 1",
                    &[&kind.label, &property, &token],
                )
                .map_err(fault)?;
            row.map(|row| row.try_get::<_, Value>("props").map_err(fault))
                .transpose()
        })
    }

    fn store_matching_platform(
        &self,
        kind: RecordKind,
        property: &str,
        value: &str,
        limit: usize,
    ) -> Result<Vec<Value>, StoreError> {
        let property = checked_property(property)?;
        self.with(|client| {
            let rows = client
                .query(
                    "SELECT props FROM store_records
                     WHERE kind = $1 AND props ->> $2 = $3
                     ORDER BY created_at DESC, seq DESC LIMIT $4",
                    &[&kind.label, &property, &value, &(limit as i64)],
                )
                .map_err(fault)?;
            rows_to_value(&rows, "props")
        })
    }

    fn store_list_platform(
        &self,
        kind: RecordKind,
        limit: usize,
    ) -> Result<Vec<Value>, StoreError> {
        self.with(|client| listed(client, kind.label, None, limit))
    }

    fn store_remove_platform(&self, kind: RecordKind, id: &str) -> Result<bool, StoreError> {
        self.with(|client| remove(client, kind.label, None, id))
    }

    fn store_update_platform(
        &self,
        kind: RecordKind,
        id: &str,
        patch: Value,
    ) -> Result<Option<Value>, StoreError> {
        self.with(|client| {
            let row = client
                .query_opt(
                    "SELECT props FROM store_records WHERE kind = $1 AND id = $2",
                    &[&kind.label, &id],
                )
                .map_err(fault)?;
            let Some(row) = row else { return Ok(None) };
            let mut record: Value = row.try_get("props").map_err(fault)?;
            if let (Some(held), Some(given)) = (record.as_object_mut(), patch.as_object()) {
                for (name, value) in given {
                    held.insert(name.clone(), value.clone());
                }
            }
            let book = record.get("book_id").and_then(Value::as_str);
            let user = record.get("user_id").and_then(Value::as_str);
            let created = record.get("created_at").and_then(Value::as_str);
            client
                .execute(
                    "UPDATE store_records SET props = $1::jsonb, book_id = $2, user_id = $3, created_at = $4
                     WHERE kind = $5 AND id = $6",
                    &[&record, &book, &user, &created, &kind.label, &id],
                )
                .map_err(fault)?;
            Ok(Some(record))
        })
    }

    fn store_shared_create(
        &self,
        kind: RecordKind,
        book_id: &str,
        props: Value,
    ) -> Result<Value, StoreError> {
        let mut props = props;
        props["book_id"] = json!(book_id);
        self.with(|client| {
            insert(client, kind.label, &props, false)?;
            Ok(props)
        })
    }

    fn store_shared_get(
        &self,
        kind: RecordKind,
        book_id: &str,
        id: &str,
    ) -> Result<Value, StoreError> {
        self.with(|client| owned(client, kind.label, book_id, id)?.ok_or(StoreError::NotFound))
    }

    fn store_shared_list(
        &self,
        kind: RecordKind,
        book_id: &str,
        limit: usize,
    ) -> Result<Vec<Value>, StoreError> {
        self.with(|client| listed(client, kind.label, Some(book_id), limit))
    }

    fn store_shared_where(
        &self,
        kind: RecordKind,
        book_id: &str,
        property: &str,
        value: Value,
    ) -> Result<Vec<Value>, StoreError> {
        let property = checked_property(property)?;
        let token = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        self.with(|client| {
            let rows = client
                .query(
                    "SELECT props FROM store_records
                     WHERE kind = $1 AND book_id = $2 AND props ->> $3 = $4
                     ORDER BY created_at DESC, seq DESC",
                    &[&kind.label, &book_id, &property, &token],
                )
                .map_err(fault)?;
            rows_to_value(&rows, "props")
        })
    }

    fn store_shared_everywhere(
        &self,
        kind: RecordKind,
        property: &str,
        value: Value,
    ) -> Result<Vec<Value>, StoreError> {
        let property = checked_property(property)?;
        let token = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        self.with(|client| {
            let rows = client
                .query(
                    "SELECT props FROM store_records
                     WHERE kind = $1 AND props ->> $2 = $3
                     ORDER BY created_at DESC, seq DESC",
                    &[&kind.label, &property, &token],
                )
                .map_err(fault)?;
            rows_to_value(&rows, "props")
        })
    }

    fn store_shared_update(
        &self,
        kind: RecordKind,
        book_id: &str,
        id: &str,
        patch: Value,
    ) -> Result<Value, StoreError> {
        self.with(|client| {
            let mut record =
                owned(client, kind.label, book_id, id)?.ok_or(StoreError::NotFound)?;
            if let (Some(held), Some(given)) = (record.as_object_mut(), patch.as_object()) {
                for (name, value) in given {
                    held.insert(name.clone(), value.clone());
                }
            }
            let book = record.get("book_id").and_then(Value::as_str);
            let user = record.get("user_id").and_then(Value::as_str);
            let created = record.get("created_at").and_then(Value::as_str);
            client
                .execute(
                    "UPDATE store_records SET props = $1::jsonb, book_id = $2, user_id = $3, created_at = $4
                     WHERE kind = $5 AND id = $6",
                    &[&record, &book, &user, &created, &kind.label, &id],
                )
                .map_err(fault)?;
            Ok(record)
        })
    }

    fn store_shared_delete(
        &self,
        kind: RecordKind,
        book_id: &str,
        id: &str,
    ) -> Result<(), StoreError> {
        self.with(|client| {
            if remove(client, kind.label, Some(book_id), id)? {
                Ok(())
            } else {
                Err(StoreError::NotFound)
            }
        })
    }
}

fn owned(
    client: &mut impl GenericClient,
    kind: &str,
    book_id: &str,
    id: &str,
) -> Result<Option<Value>, StoreError> {
    let row = client
        .query_opt(
            "SELECT props FROM store_records WHERE kind = $1 AND id = $2 AND book_id = $3",
            &[&kind, &id, &book_id],
        )
        .map_err(fault)?;
    row.map(|row| row.try_get::<_, Value>("props").map_err(fault))
        .transpose()
}

fn listed(
    client: &mut impl GenericClient,
    kind: &str,
    book_id: Option<&str>,
    limit: usize,
) -> Result<Vec<Value>, StoreError> {
    let rows = client
        .query(
            "SELECT props FROM store_records
             WHERE kind = $1 AND ($2::text IS NULL OR book_id = $2)
             ORDER BY created_at DESC, seq DESC LIMIT $3",
            &[&kind, &book_id, &(limit as i64)],
        )
        .map_err(fault)?;
    rows_to_value(&rows, "props")
}

fn children_of(
    client: &mut impl GenericClient,
    child: RecordKind,
    parent: &str,
) -> Result<Vec<Value>, StoreError> {
    let rows = client
        .query(
            "SELECT c.props FROM store_edges e
             JOIN store_records c ON c.id = e.to_id AND c.kind = $1
             WHERE e.from_id = $2 AND e.label = $3
             ORDER BY c.seq ASC",
            &[&child.label, &parent, &child.edge],
        )
        .map_err(fault)?;
    rows_to_value(&rows, "props")
}

fn account_id(
    client: &mut impl GenericClient,
    book_id: &str,
    account_number: &str,
) -> Result<Option<String>, StoreError> {
    let row = client
        .query_opt(
            "SELECT id FROM store_records
             WHERE kind = $1 AND book_id = $2 AND props ->> 'account_number' = $3
             ORDER BY created_at DESC, seq DESC LIMIT 1",
            &[&LEDGER_ACCOUNT.label, &book_id, &account_number],
        )
        .map_err(fault)?;
    row.map(|row| row.try_get::<_, String>("id").map_err(fault))
        .transpose()
}

fn remove(
    client: &mut impl GenericClient,
    kind: &str,
    book_id: Option<&str>,
    id: &str,
) -> Result<bool, StoreError> {
    let mut transaction = client.transaction().map_err(fault)?;
    let removed = transaction
        .execute(
            "DELETE FROM store_records WHERE kind = $1 AND id = $2 AND ($3::text IS NULL OR book_id = $3)",
            &[&kind, &id, &book_id],
        )
        .map_err(fault)?;
    if removed == 0 {
        return Ok(false);
    }
    transaction
        .execute(
            "DELETE FROM store_edges WHERE from_id = $1 OR to_id = $1",
            &[&id],
        )
        .map_err(fault)?;
    transaction.commit().map_err(fault)?;
    Ok(true)
}

/// The kinds this module names in its queries, checked at compile time by being used: a rename in the ledger's own
/// vocabulary would otherwise break a query that nothing reads until a posting is walked.
const _: [RecordKind; 2] = [JOURNAL_ENTRY, JOURNAL_LINE];
