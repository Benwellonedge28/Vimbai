#![forbid(unsafe_code)]
//! The offline store's two descriptions of itself, checked against each other.
//!
//! `database_helper.dart` describes each table for a phone that installs today in `_onCreate`, and separately in
//! `_onUpgrade` for a phone that installed earlier. The two drifted: `journal_entries` gained `book_id` for upgraded
//! installs only, so a fresh install could not keep a journal entry at all and every existing phone worked. These tests
//! are what notices, over the real file and over the shapes that make the scan wrong if it guesses.

use std::path::Path;

use xtask::offline_schema;

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root, two levels above rust/xtask")
}

#[test]
fn everything_the_app_writes_is_a_column_a_fresh_install_has() {
    let missing = offline_schema::missing_on_a_fresh_install_at(repo())
        .expect("the offline store is readable");
    assert!(
        missing.is_empty(),
        "a phone installing today would fail on these: {missing:?}"
    );
}

#[test]
fn the_scan_actually_reads_something() {
    let source = std::fs::read_to_string(repo().join(offline_schema::HELPER)).expect("the file");
    let created = offline_schema::created_tables(&source);
    let written = offline_schema::written_columns(&source);
    assert!(
        created.len() >= 5,
        "the create path describes the tables: {created:?}"
    );
    assert!(
        written.contains_key("journal_entries"),
        "the app writes journal entries: {written:?}"
    );
    // the table this was written for: a column added by the upgrade path is not a column a fresh install has
    assert!(
        created["accounts"].contains("book_id"),
        "accounts has book_id in the create path too"
    );
}

#[test]
fn a_column_only_the_upgrade_path_adds_is_reported() {
    // the shape of the real fault, in the smallest form that has it: `_onCreate` gives the table three columns, the
    // upgrade path adds a fourth, and the app writes all four - so a phone that installed today fails and every older
    // phone works.
    let source = r#"
      await db.execute('''
            CREATE TABLE journal_entries(
              id TEXT PRIMARY KEY,
              entry_date TEXT,
              description TEXT
            )
          ''');
      if (oldVersion < 4) {
        await db.execute('ALTER TABLE journal_entries ADD COLUMN book_id TEXT');
      }
      await db.insert('journal_entries', {
        'id': entryId,
        'entry_date': entry.entryDate.toIso8601String(),
        'description': entry.description,
        'book_id': BookContext.instance.current?.id,
      });
    "#;
    let missing = offline_schema::missing_on_a_fresh_install(source);
    assert_eq!(
        missing,
        vec![("journal_entries".to_string(), "book_id".to_string())],
        "the upgrade path must not stand in for the create path"
    );
}

#[test]
fn a_table_nothing_creates_is_named_as_the_whole_table() {
    let source = r#"
      await db.insert('offline_cache', {'key': 'k', 'value': 'v'});
    "#;
    assert_eq!(
        offline_schema::missing_on_a_fresh_install(source),
        vec![("offline_cache".to_string(), "*the table*".to_string())],
        "a missing table is a worse fault than a missing column, and reads differently"
    );
}

#[test]
fn a_table_constraint_is_not_read_as_a_column() {
    let source = r#"
      CREATE TABLE journal_lines(
        id TEXT PRIMARY KEY,
        journal_entry_id TEXT,
        debit REAL,
        FOREIGN KEY (journal_entry_id) REFERENCES journal_entries(id) ON DELETE CASCADE
      )
    "#;
    let created = offline_schema::created_tables(source);
    let lines = &created["journal_lines"];
    assert!(lines.contains("debit"), "{lines:?}");
    assert!(
        !lines
            .iter()
            .any(|column| column.eq_ignore_ascii_case("foreign")),
        "the constraint is not a column: {lines:?}"
    );
}

#[test]
fn a_write_is_read_without_its_values_being_mistaken_for_columns() {
    // the values contain commas, quotes and a call with its own parentheses; only the keys are columns
    let source = r#"
      CREATE TABLE budget_items(
        id TEXT PRIMARY KEY,
        budget_id TEXT,
        category TEXT,
        account_number TEXT
      )
      await db.insert('budget_items', {
        'id': uuid.v4(),
        'budget_id': item.budgetId ?? '',
        'category': item.category.contains(',') ? 'split' : 'single',
        'account_number': item.accountNumber,
      });
    "#;
    assert!(
        offline_schema::missing_on_a_fresh_install(source).is_empty(),
        "every key is a column here"
    );
}
