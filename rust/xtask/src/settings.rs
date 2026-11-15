//! Every environment variable a deployment sets is a variable this code reads.
//!
//! The estate has been bitten by this from both sides and in ways no single file shows. A Secret declared `NEO4J_PASSWORD`
//! while the pod read `neo4j-password`, so the pod never started. A manifest set `VIMBAI_SYNC_ADDR` that the sync server never
//! looked at, so the container port it declared was never the port anything listened on. Seven service URLs were configured for
//! a gateway that routes from the catalogue instead, left pointing at services that no longer exist.
//!
//! A variable nothing reads is not configuration. It is either a typo, a leftover, or a promise the code does not keep, and in
//! every case the manifest reads as though the behaviour is there. So: a name a manifest or compose file sets has to appear as a
//! quoted name somewhere in the source that would read it.
//!
//! The exception is a variable read by an image this repository does not build - a database, a collector, a cache. Those are
//! named with the reason they are not ours, and an entry that stops being configured is a failure too, so the list cannot rot.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::repository;

/// Variables read by images this repository does not build, and why the source here cannot name them.
pub const EXTERNAL: &[(&str, &str)] = &[
    (
        "NEO4J_AUTH",
        "read by the neo4j image this file runs, which this repository pulls rather than builds",
    ),
    (
        "NEO4J_PLUGINS",
        "read by the neo4j image this file runs, which this repository pulls rather than builds",
    ),
    (
        "GF_AUTH_ANONYMOUS_ENABLED",
        "read by the grafana image in the observability variant, which this repository pulls rather than builds",
    ),
    (
        "GF_SECURITY_ADMIN_PASSWORD",
        "read by the grafana image in the observability variant, which this repository pulls rather than builds",
    ),
    (
        "COLLECTOR_OTLP_ENABLED",
        "read by the jaegertracing/all-in-one image, which this repository pulls rather than builds",
    ),
    (
        "OTEL_EXPORTER_JAEGER_ENDPOINT",
        "read by the jaeger image itself; our own binaries do not speak to it",
    ),
];

/// Names written into a manifest's `env:` block, keyed by name, with the files that set them.
pub fn configured(root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut configured: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (path, text) in manifest_files(root) {
        let relative = relative(root, &path);
        for name in env_names(&text) {
            configured.entry(name).or_default().push(relative.clone());
        }
    }
    configured
}

/// Names that appear as a quoted string in the source that would read one.
pub fn named_in_source(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (_, text) in source_files(root) {
        for name in quoted_names(&text) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// Every problem the rule finds, empty when the estate agrees with itself.
pub fn problems(root: &Path) -> Vec<String> {
    let configured = configured(root);
    if configured.is_empty() {
        return vec![
            "no manifest sets an environment variable, so this scan is reading nothing".to_string(),
        ];
    }
    let named = named_in_source(root);
    if named.is_empty() {
        return vec![
            "no source file names an environment variable, so this scan is reading nothing"
                .to_string(),
        ];
    }
    let mut problems = Vec::new();
    for (name, files) in &configured {
        if named.contains(name) {
            continue;
        }
        if EXTERNAL.iter().any(|(external, _)| external == name) {
            continue;
        }
        problems.push(format!(
            "{} sets `{name}`, which no source file here reads: either the code should read it or the manifest should stop setting it",
            files.join(", ")
        ));
    }
    problems
}

/// What is wrong with the excuse list itself: an entry that excuses nothing, or one that hides a name the source reads.
///
/// Separate from `problems` because that half answers a question about any directory, and this half is about this estate's
/// list of images it does not build. A test that builds its own little directory would otherwise be told the list is stale.
pub fn excuse_problems(root: &Path) -> Vec<String> {
    let configured = configured(root);
    let named = named_in_source(root);
    let mut problems = Vec::new();
    for (name, reason) in EXTERNAL {
        if !configured.contains_key(*name) {
            problems.push(format!(
                "`{name}` is excused as {reason}, but nothing sets it any more, so the excuse is stale"
            ));
        }
        // an excuse for a name the source does read would hide a real variable behind an allowlist
        if named.contains(&name.to_string()) {
            problems.push(format!(
                "`{name}` is excused as not ours, but a source file here reads it, so nothing should be excused"
            ));
        }
    }
    problems
}

/// The `name:` entries of every `env:` block in a YAML text, in declaration order.
///
/// The manifests spell an environment variable either as `- name: X` under a container's `env:`, or as `X: value` in a
/// compose `environment:` map. Both are read here, and an indented line that closes the block ends it.
pub fn env_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut block: Option<usize> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if let Some(at) = block {
            // a list item under `env:` may sit at the same indent as the key itself
            let inside = indent > at || (indent == at && trimmed.starts_with('-'));
            if !inside {
                block = None;
            }
        }
        if trimmed.starts_with("env:") || trimmed.starts_with("environment:") {
            block = Some(indent);
            continue;
        }
        let Some(_) = block else { continue };
        if let Some(rest) = trimmed.strip_prefix("- name:") {
            let name = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if is_variable_name(&name) {
                names.push(name);
            }
            continue;
        }
        if let Some((key, _)) = trimmed.split_once(':') {
            let key = key.trim().trim_matches('"').trim_matches('\'');
            if is_variable_name(key) {
                names.push(key.to_string());
            }
        }
    }
    names
}

/// Names spelled as an environment variable: upper case, digits and underscores, and not a bare word like `TRUE`.
pub fn is_variable_name(name: &str) -> bool {
    name.len() >= 3
        && name.starts_with(|character: char| character.is_ascii_uppercase())
        && name.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
        && name.chars().any(|character| character.is_ascii_uppercase())
}

/// Quoted upper-case names in a source text, which is how any of the three languages here spells a variable it reads.
pub fn quoted_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for character in text.chars() {
        match quote {
            None => {
                if character == '"' || character == '\'' || character == '`' {
                    quote = Some(character);
                    current.clear();
                }
            }
            Some(delimiter) => {
                if character == delimiter {
                    if is_variable_name(&current) {
                        names.push(current.clone());
                    }
                    quote = None;
                    current.clear();
                } else if character == '\n' || character == '\r' {
                    // an unterminated quote is not a literal; drop it rather than reading across lines
                    quote = None;
                    current.clear();
                } else if current.chars().count() < 64 {
                    current.push(character);
                } else {
                    quote = None;
                    current.clear();
                }
            }
        }
    }
    names
}

fn manifest_files(root: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    for path in under(root, "infrastructure", &["yaml", "yml"]) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        files.push((path, text));
    }
    files
}

fn source_files(root: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    for (tree, extension) in [
        ("rust", "rs"),
        ("gateway", "go"),
        ("apps/flutter/lib", "dart"),
    ] {
        for path in under(root, tree, &[extension]) {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            files.push((path, text));
        }
    }
    files
}

/// Every file under one tree whose extension is one of `extensions`, from the one walk the whole crate shares.
fn under(root: &Path, tree: &str, extensions: &[&str]) -> Vec<PathBuf> {
    let prefix = format!("{}/", root.join(tree).to_string_lossy());
    repository::files(root)
        .into_iter()
        .filter(|path| path.starts_with(&prefix))
        .filter(|path| {
            extensions
                .iter()
                .any(|extension| path.ends_with(&format!(".{extension}")))
        })
        // a test that names a variable, and this rule's own table of excused names, are not code reading one
        .filter(|path| !path.contains("/tests/") && !path.ends_with("_test.rs"))
        .filter(|path| !path.ends_with("xtask/src/settings.rs"))
        .map(PathBuf::from)
        .collect()
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}
