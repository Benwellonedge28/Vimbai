//! Where the code lives, and what it is allowed to be.
//!
//! The services used to be separate Python containers that each carried a copy of the same models, the same database
//! access and the same arithmetic. Porting them one at a time into the shared store crate would have repeated that
//! mistake in Rust: fourteen modules side by side in the adapter, each reachable only through it. They live in one
//! crate per business domain now, and this is what keeps it that way.
//!
//! The crate list comes from the catalogue rather than from a second list, so a service added to the router is checked
//! here without anyone remembering to add it.
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::catalog::{self, Row};

/// The store's own work: the record shapes, the Book scoping, the driver and the helpers their tests use. Nothing here
/// computes a business figure.
const STORE_MODULES: [&str; 13] = [
    "amounts.rs",
    // the columns every backend indexes a record by, shared so two of them cannot disagree
    "columns.rs",
    // moving records between stores: machinery about where a record lives, which is the store's business and not a
    // service's. It knows a record's kind and properties and nothing about what either means.
    "dump.rs",
    "fake.rs",
    "file.rs",
    "lib.rs",
    "neo4j_client.rs",
    // the deployment backend, for the same reason as sqlite.rs: a database is machinery
    "postgres.rs",
    "query.rs",
    "read.rs",
    "shared.rs",
    // a backend, like file.rs and neo4j_client.rs: it knows where a record lives and nothing about what it means, and it
    // answers the same doors the others do. A device store is machinery, not a service.
    "sqlite.rs",
    "write.rs",
];

/// The modules the store's `lib.rs` may declare: the same machinery under its module names.
const STORE_DECLARATIONS: [&str; 12] = [
    "amounts",
    "columns",
    "dump",
    "fake",
    "file",
    "neo4j_client",
    "postgres",
    "query",
    "read",
    "shared",
    "sqlite",
    "write",
];

fn store(root: &Path) -> std::path::PathBuf {
    root.join("rust/crates/adapters/database/src")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|problem| {
        panic!("{} is unreadable: {problem}", path.display());
    })
}

fn rust_services(root: &Path) -> Vec<Row> {
    catalog::rows(root)
        .into_iter()
        .filter(|row| row.engine == "rust")
        .collect()
}

/// The store grew a service module again.
pub fn store_service_modules(root: &Path) -> Vec<String> {
    let directory = store(root);
    let mut present: Vec<String> = std::fs::read_dir(&directory)
        .unwrap_or_else(|problem| panic!("{} is unreadable: {problem}", directory.display()))
        .filter_map(Result::ok)
        .map(|file| file.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".rs"))
        .collect();
    present.sort();
    present
        .into_iter()
        .filter(|name| !STORE_MODULES.contains(&name.as_str()))
        .collect()
}

/// The store re-exports a domain module, so its arithmetic is reachable only through the adapter.
pub fn store_declared_domains(root: &Path) -> Vec<String> {
    let text = read(&store(root).join("lib.rs"));
    let mut declared = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start_matches(char::is_whitespace);
        let Some(rest) = trimmed.strip_prefix("pub mod ") else {
            continue;
        };
        let Some(name) = rest.strip_suffix(';') else {
            continue;
        };
        if !STORE_DECLARATIONS.contains(&name) {
            declared.push(name.to_string());
        }
    }
    declared.sort();
    declared
}

/// A service the catalogue calls Rust whose crate does not exist.
pub fn services_without_a_crate(root: &Path) -> Vec<String> {
    rust_services(root)
        .into_iter()
        .filter(|row| !root.join(&row.target).join("Cargo.toml").is_file())
        .map(|row| format!("{} -> {}", row.name, row.target))
        .collect()
}

/// A service the catalogue calls Rust that the crate housing it never names.
pub fn services_unnamed_by_their_crate(root: &Path) -> Vec<String> {
    rust_services(root)
        .into_iter()
        .filter(|row| {
            let lib = root.join(&row.target).join("src/lib.rs");
            lib.is_file() && !read(&lib).contains(&row.name)
        })
        .map(|row| format!("{} is not named in {}/src/lib.rs", row.name, row.target))
        .collect()
}

/// Crates with no `#![forbid(unsafe_code)]`, swept across the whole workspace rather than the domains alone.
pub fn crates_without_the_safety_attribute(root: &Path) -> Vec<String> {
    let mut gaps = Vec::new();
    let rust = root.join("rust");
    let mut stack = vec![rust.clone()];
    while let Some(directory) = stack.pop() {
        if directory.file_name().is_some_and(|name| name == "target") {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut has_manifest = false;
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|name| name == "Cargo.toml") {
                has_manifest = true;
            }
        }
        if !has_manifest {
            continue;
        }
        let main = directory.join("src/main.rs");
        let lib = directory.join("src/lib.rs");
        let entry = if main.is_file() { main } else { lib };
        if !entry.is_file() {
            continue;
        }
        if !read(&entry).contains("#![forbid(unsafe_code)]") {
            let relative = directory
                .strip_prefix(&rust)
                .unwrap_or(&directory)
                .to_string_lossy()
                .into_owned();
            gaps.push(relative);
        }
    }
    gaps.sort();
    gaps
}

/// A domain whose services live in more than one crate.
pub fn split_domains(root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut by_domain: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in rust_services(root) {
        by_domain.entry(row.domain).or_default().insert(row.target);
    }
    by_domain
        .into_iter()
        .filter(|(_, targets)| targets.len() > 1)
        .map(|(domain, targets)| (domain, targets.into_iter().collect()))
        .collect()
}

/// The costing services the crate fails to name, which is the consolidation this whole rule exists for.
pub fn costing_services_unnamed(root: &Path) -> Vec<String> {
    let lib = root.join("rust/crates/domains/costing/src/lib.rs");
    let text = read(&lib);
    let mut unnamed: Vec<String> = rust_services(root)
        .into_iter()
        .filter(|row| row.domain == "costing" && !text.contains(&row.name))
        .map(|row| row.name)
        .collect();
    unnamed.sort();
    unnamed
}
