#![forbid(unsafe_code)]

//! The crates the workspace builds, against the crates that exist.
//!
//! A crate directory that is not a workspace member is invisible: `cargo test --workspace` never builds it, clippy never reads
//! it, and the safety attribute it may or may not carry is never looked at. It compiles only if someone remembers to cd into it.
//! That is a silent hole, and the same shape as the two defects already found in the pipelines - a job that never starts, a test
//! suite that skips - so it is worth a gate rather than a habit.

use std::path::Path;

/// The paths listed under `members` in the workspace manifest.
pub fn members(root: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(root.join("rust/Cargo.toml")) else {
        return Vec::new();
    };
    list_under(&text, "members")
}

/// The paths listed under `exclude`: crates that are deliberately their own workspace.
pub fn excluded(root: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(root.join("rust/Cargo.toml")) else {
        return Vec::new();
    };
    list_under(&text, "exclude")
}

/// Every crate directory on disk, relative to `rust/`, sorted. The workspace manifest itself is not a crate.
pub fn crates_on_disk(root: &Path) -> Vec<String> {
    let mut found: Vec<String> = crate::repository::files(&root.join("rust"))
        .into_iter()
        .filter(|file| file.ends_with("/Cargo.toml"))
        .filter_map(|file| {
            let manifest = file.trim_end_matches("/Cargo.toml");
            let directory = manifest.strip_prefix(&format!("{}/", root.join("rust").display()))?;
            Some(directory.to_string())
        })
        .collect();
    found.sort();
    found
}

/// A crate on disk that the workspace does not build, or a member that is not there.
pub fn not_in_the_workspace(root: &Path) -> Vec<String> {
    let crates = crates_on_disk(root);
    if crates.is_empty() {
        return vec!["there are no crates under rust/ to check against the workspace".to_string()];
    }
    let members = members(root);
    if members.is_empty() {
        return vec!["rust/Cargo.toml lists no workspace members".to_string()];
    }
    let excluded = excluded(root);
    let mut problems = Vec::new();
    for directory in &crates {
        if members.contains(directory) || excluded.contains(directory) {
            continue;
        }
        let manifest = root.join("rust").join(directory).join("Cargo.toml");
        let own_workspace = std::fs::read_to_string(&manifest)
            .map(|text| text.contains("[workspace]"))
            .unwrap_or(false);
        problems.push(if own_workspace {
            format!(
                "rust/{directory} declares its own workspace and is not in `exclude`, so nothing here builds it"
            )
        } else {
            format!(
                "rust/{directory} is a crate the workspace does not list, so `cargo test --workspace` never builds it"
            )
        });
    }
    for member in members {
        if !crates.contains(&member) {
            problems.push(format!(
                "the workspace lists rust/{member} and there is no crate there"
            ));
        }
    }
    problems
}

/// The quoted strings inside `key = [ ... ]` in a manifest.
fn list_under(manifest: &str, key: &str) -> Vec<String> {
    let Some(after) = manifest.split(&format!("{key} = [")).nth(1) else {
        return Vec::new();
    };
    let Some(block) = after.split(']').next() else {
        return Vec::new();
    };
    block
        .split('"')
        .skip(1)
        .step_by(2)
        .map(|entry| entry.trim_end_matches('/').to_string())
        .filter(|entry| !entry.is_empty())
        .collect()
}
