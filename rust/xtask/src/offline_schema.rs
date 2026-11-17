//! The app's offline store: what it creates, against what it writes.
//!
//! `database_helper.dart` describes each table twice - once in `_onCreate`, for a phone that installs today, and once
//! more in `_onUpgrade`, as the changes a phone that installed earlier needs. Those two drift silently, and the drift
//! only shows up on one of the two phones: `journal_entries` grew a `book_id` for upgraded installs and never got one for
//! fresh ones, so a phone that installed today could not keep a journal entry at all while every existing phone worked.
//! Nothing in the app or in CI noticed, because the only dart that would have failed was the one nobody ran.
//!
//! This module answers the question that catches it without needing a dart toolchain: every column the app writes into a
//! table must be a column `_onCreate` gives that table. The upgrade path is not a substitute for the create path, and the
//! check is a plain library function so the worker or an editor hook can call it rather than reimplementing the scan.
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The file that holds the offline store's tables, relative to the repository root.
pub const HELPER: &str = "apps/flutter/lib/core/local_db/database_helper.dart";

/// A table as `_onCreate` describes it.
pub type Created = BTreeMap<String, BTreeSet<String>>;

/// What the app writes into each table, as the keys of each `db.insert('<table>', {...})`.
pub type Written = BTreeMap<String, BTreeSet<String>>;

/// Read every `CREATE TABLE` in the file's create path: the table name and the columns it declares.
pub fn created_tables(source: &str) -> Created {
    let mut tables: Created = BTreeMap::new();
    let mut rest = source;
    while let Some(start) = rest.find("CREATE TABLE") {
        rest = &rest[start + "CREATE TABLE".len()..];
        // `CREATE TABLE journal_entries(` - the name runs to the parenthesis
        let Some(open) = rest.find('(') else { break };
        let name = rest[..open]
            .trim()
            .trim_end_matches(',')
            .trim()
            .trim_matches('"')
            .to_string();
        let body = &rest[open + 1..];
        let Some(close) = matching_paren(body) else {
            break;
        };
        let mut columns = BTreeSet::new();
        for line in body[..close].lines() {
            let line = line.trim().trim_end_matches(',');
            if line.is_empty() || line.starts_with("--") {
                continue;
            }
            let upper = line.to_ascii_uppercase();
            // a table-level constraint is not a column
            if upper.starts_with("FOREIGN KEY")
                || upper.starts_with("PRIMARY KEY")
                || upper.starts_with("UNIQUE")
                || upper.starts_with("CHECK")
            {
                continue;
            }
            if let Some(first) = line.split_whitespace().next() {
                columns.insert(first.trim_matches('"').to_string());
            }
        }
        tables.entry(name).or_default().extend(columns);
        rest = &body[close + 1..];
    }
    tables
}

/// Where the first create block ends, so an `ALTER TABLE` further down is not read as part of it.
fn matching_paren(body: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, character) in body.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Every column the app writes, table by table, from the `db.insert` / `db.update` calls in the file.
pub fn written_columns(source: &str) -> Written {
    let mut written: Written = BTreeMap::new();
    for table in insertions(source) {
        written.entry(table.0).or_default().extend(table.1);
    }
    written
}

fn insertions(source: &str) -> Vec<(String, BTreeSet<String>)> {
    let mut found = Vec::new();
    let mut rest = source;
    // both a new row and a change to one name their columns the same way
    while let Some(start) = rest.find(".insert(") {
        rest = &rest[start + ".insert(".len()..];
        let Some(quote) = rest.find('\'') else { break };
        let after = &rest[quote + 1..];
        let Some(end) = after.find('\'') else { break };
        let table = after[..end].to_string();
        let tail = &after[end + 1..];
        // the map begins at the first brace; its keys are the columns
        let Some(brace) = tail.find('{') else {
            continue;
        };
        let Some(close) = matching_brace(&tail[brace + 1..]) else {
            continue;
        };
        let body = &tail[brace + 1..brace + 1 + close];
        let mut columns = BTreeSet::new();
        for part in body.split(',') {
            let part = part.trim();
            let Some(colon) = part.find(':') else {
                continue;
            };
            let key = part[..colon].trim().trim_matches('\'').trim();
            if !key.is_empty() && !key.contains(' ') && !key.starts_with("//") {
                columns.insert(key.to_string());
            }
        }
        found.push((table, columns));
        rest = &tail[brace + 1 + close..];
    }
    found
}

fn matching_brace(body: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, character) in body.char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// The writes that a phone installing today would fail on: a column the app writes into a table that the create path
/// never gives that table. Sorted, so the same finding reads the same way twice.
pub fn missing_on_a_fresh_install(source: &str) -> Vec<(String, String)> {
    let created = created_tables(source);
    let written = written_columns(source);
    let mut missing = Vec::new();
    for (table, columns) in &written {
        let Some(declared) = created.get(table) else {
            // a table nothing creates is a worse fault than a missing column, and it is named as such
            missing.push((table.clone(), "*the table*".to_string()));
            continue;
        };
        for column in columns {
            if !declared.contains(column) {
                missing.push((table.clone(), column.clone()));
            }
        }
    }
    missing.sort();
    missing
}

/// The same answer for a file on disk.
pub fn missing_on_a_fresh_install_at(repo: &Path) -> Result<Vec<(String, String)>, String> {
    let path = repo.join(HELPER);
    let source =
        std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(missing_on_a_fresh_install(&source))
}
