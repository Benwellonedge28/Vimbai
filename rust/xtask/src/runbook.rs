#![forbid(unsafe_code)]
//! What an operator is told, checked against what the code actually does.
//!
//! A runbook is read at the worst moment: something is down, and the page has to be true. The failure it invites is quiet -
//! a setting renamed in the code and left behind in the page sends an operator to a variable nothing reads, and they
//! conclude the platform ignores them. So the settings a runbook names are checked against the settings the source actually
//! asks for, and a page that names one nothing reads fails the build.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The settings the code reads, gathered from the source rather than listed here.
///
/// A name counts when it appears in the source as a string, because that is how a setting is handed to whatever reads the
/// environment - `env::var("NAME")` directly, or `read("NAME")` where one small helper does the reading. A name in a comment
/// does not count: a page and a comment agreeing with each other would prove nothing.
pub fn settings_the_code_reads(root: &Path) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for file in rust_files(&root.join("rust")) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            names.extend(quoted_settings(line));
        }
    }
    names
}

/// The settings a runbook names, so a caller can say what is documented.
pub fn settings_the_runbook_names(root: &Path) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let directory = root.join("docs").join("operations");
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return names;
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    files.sort();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for word in text.split('`').skip(1).step_by(2) {
            let trimmed = word.trim();
            if is_setting_shaped(trimmed) {
                names.insert(trimmed.to_string());
            }
        }
    }
    names
}

/// A runbook names a setting the code never reads.
pub fn unknown_settings(root: &Path) -> Vec<String> {
    let read = settings_the_code_reads(root);
    settings_the_runbook_names(root)
        .into_iter()
        .filter(|named| !read.contains(named))
        .collect()
}

/// The runbooks are empty, which means an operator with a broken engine has nothing to read.
pub fn runbook_is_missing(root: &Path) -> bool {
    settings_the_runbook_names(root).is_empty()
}

fn is_setting_shaped(word: &str) -> bool {
    let Some((prefix, rest)) = word.split_once('_') else {
        return false;
    };
    if !matches!(prefix, "VIMBAI" | "NEO4J") {
        return false;
    }
    !rest.is_empty()
        && rest.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

/// Every quoted setting-shaped name on a line.
fn quoted_settings(line: &str) -> Vec<String> {
    line.split('"')
        .skip(1)
        .step_by(2)
        .map(str::trim)
        .filter(|word| is_setting_shaped(word))
        .map(str::to_string)
        .collect()
}

fn rust_files(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                // a build directory is nobody's source, and tests/ fixtures are still source a reader may be sent to
                if path.file_name().is_some_and(|name| name == "target") {
                    continue;
                }
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
