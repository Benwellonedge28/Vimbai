//! Does the engine answer every address the gateway publishes?
//!
//! A service that has been retired keeps its address: the gateway entry survives, pointed at the engine. That only works
//! if the engine registers the address the gateway forwards, and whether the gateway forwards the service's prefix or
//! strips it is per entry, so the two have to agree. Nothing checked that until the Python audit; nine services had
//! drifted, most of them on parameterised paths a fixed-path survey never looked at.
//!
//! The addresses live in the retired services' last `main.py`, which is why this reads them back out of git: the
//! directory is gone, but the promise the gateway makes was written there, and a promise is checked against what was
//! actually said rather than against the current router.
//!
//! This crate has no regex dependency, so the scanners below are hand-written against the two shapes that appear in the
//! sources: `@app.get("/path")` in the old services, and `.route("…", get(handler))` / `.route(&format!("{prefix}…"))`
//! in the engine. Both are read by walking the text, not by matching a pattern that a reflow could silently break.
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::catalog::repo_root;

/// Where the engine's route tables live, relative to the root.
const API: &str = "rust/apps/api-server/src";

/// The two mounts the accounting table hangs itself off, used when a file does not say otherwise.
const DEFAULT_MOUNTS: [&str; 2] = ["/accounting", "/records/accounting"];

/// Addresses that are not part of a service's published contract.
const SKIP: [&str; 3] = ["", "/", "/health"];

/// One gateway entry, read from the router's own configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub url: String,
    pub strip_prefix: bool,
}

/// A path with its parameters blanked, so `{id}` and `:id` compare equal.
pub fn shape(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut characters = path.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '{' => {
                for next in characters.by_ref() {
                    if next == '}' {
                        break;
                    }
                }
                out.push_str("{}");
            }
            ':' => {
                if characters
                    .peek()
                    .is_some_and(|next| next.is_alphabetic() || *next == '_')
                {
                    while characters
                        .peek()
                        .is_some_and(|next| next.is_alphanumeric() || *next == '_')
                    {
                        characters.next();
                    }
                    out.push_str("{}");
                } else {
                    out.push(':');
                }
            }
            other => out.push(other),
        }
    }
    let trimmed = out.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_skipped(path: &str) -> bool {
    SKIP.contains(&path.trim_end_matches('/')) || SKIP.contains(&path)
}

/// Every gateway entry, in the order the router serves them.
pub fn entries(root: &Path) -> Vec<Entry> {
    let path = root.join("gateway/go/config/services.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|problem| {
        panic!("{} is unreadable: {problem}", path.display());
    });
    let document: Value = serde_json::from_str(&text)
        .unwrap_or_else(|problem| panic!("{} is not JSON: {problem}", path.display()));
    let services = document
        .get("services")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{} has no services array", path.display()));
    services
        .iter()
        .map(|entry| Entry {
            name: string_of(entry, "name"),
            path: string_of(entry, "path"),
            url: string_of(entry, "url"),
            strip_prefix: entry
                .get("strip_prefix")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
        .collect()
}

fn string_of(entry: &Value, field: &str) -> String {
    entry
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("a gateway entry is missing {field}: {entry}"))
        .to_string()
}

/// The engine's sources, file name order, so a scan is reproducible.
fn engine_sources(root: &Path) -> Vec<String> {
    let directory = root.join(API);
    let mut names: Vec<String> = std::fs::read_dir(&directory)
        .unwrap_or_else(|problem| panic!("{} is unreadable: {problem}", directory.display()))
        .filter_map(Result::ok)
        .map(|file| file.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".rs"))
        .collect();
    names.sort();
    names
        .into_iter()
        .filter_map(|name| std::fs::read_to_string(directory.join(&name)).ok())
        .collect()
}

/// The prefixes a file hangs its routes off, read from `for prefix in ["/a", "/b"]`.
fn mounts_for(text: &str) -> Vec<String> {
    let Some(start) = text.find("for prefix in [") else {
        return Vec::new();
    };
    let rest = &text[start + "for prefix in [".len()..];
    let Some(end) = rest.find(']') else {
        return Vec::new();
    };
    let mut mounts = Vec::new();
    for found in rest[..end].split(',') {
        let name = found.trim().trim_matches('"');
        if name.starts_with('/') {
            mounts.push(name.to_string());
        }
    }
    mounts
}

/// The arguments of the call whose opening bracket sits at `open_at`.
fn call_arguments(text: &str, open_at: usize) -> Option<&str> {
    let mut depth = 0usize;
    for (offset, character) in text[open_at..].char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let from = open_at + 1;
                    return Some(&text[from..open_at + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The methods a route's arguments bind, read from `get(handler).put(handler)`.
fn methods_of(arguments: &str) -> BTreeSet<String> {
    let mut methods = BTreeSet::new();
    for method in ["get", "post", "put", "delete", "patch"] {
        let needle = format!("{method}(");
        if arguments.contains(&needle) {
            methods.insert(method.to_string());
        }
    }
    // `published("name")` serves a calculator at the address its Python service published, and takes POST only.
    if arguments.contains("published(") {
        methods.insert("post".to_string());
    }
    methods
}

/// The first string literal in an expression, with its escape-free contents.
fn first_literal(arguments: &str) -> Option<String> {
    let start = arguments.find('"')?;
    let rest = &arguments[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// The engine's routes as (method, path) pairs, read out of its sources.
pub fn engine_method_routes(root: &Path) -> BTreeSet<(String, String)> {
    let mut seen = BTreeSet::new();
    for text in engine_sources(root) {
        let mounts = {
            let declared = mounts_for(&text);
            if declared.is_empty() {
                DEFAULT_MOUNTS
                    .iter()
                    .map(|mount| mount.to_string())
                    .collect()
            } else {
                declared
            }
        };
        let mut from = 0usize;
        while let Some(offset) = text[from..].find(".route(") {
            let open_at = from + offset + ".route".len();
            from = open_at;
            let Some(arguments) = call_arguments(&text, open_at) else {
                continue;
            };
            let methods = methods_of(arguments);
            if methods.is_empty() {
                continue;
            }
            let Some(literal) = first_literal(arguments) else {
                continue;
            };
            let is_format = arguments[..arguments.find('"').unwrap_or(0)].contains("format!");
            if !is_format {
                for method in &methods {
                    seen.insert((method.clone(), shape(&literal)));
                }
                continue;
            }
            // A `format!` route either splices the mount in (`"{prefix}/accounts"`) or is already whole.
            if let Some(suffix) = literal.strip_prefix("{prefix}") {
                for mount in &mounts {
                    for method in &methods {
                        seen.insert((method.clone(), shape(&format!("{mount}{suffix}"))));
                    }
                }
            } else {
                for method in &methods {
                    seen.insert((method.clone(), shape(&literal)));
                }
            }
        }
    }
    seen
}

/// The engine's addresses alone, method aside.
pub fn engine_routes(root: &Path) -> BTreeSet<String> {
    engine_method_routes(root)
        .into_iter()
        .map(|(_, path)| path)
        .collect()
}

/// The last version of a retired service's `main.py`, read back out of git.
pub fn last_main(root: &Path, service: &str) -> Option<String> {
    let path = format!("{service}/main.py");
    let commit = git(root, &["log", "-1", "--format=%H", "--", path.as_str()])?
        .trim()
        .to_string();
    if commit.is_empty() {
        return None;
    }
    let parents = git(root, &["rev-list", "--parents", "-n", "1", commit.as_str()])?;
    let parent = parents.split_whitespace().nth(1)?.to_string();
    git(root, &["show", &format!("{parent}:{path}")])
}

fn git(root: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Every (method, path) a retired service published, read back out of git.
pub fn published(root: &Path, service: &str) -> Vec<(String, String)> {
    let Some(text) = last_main(root, service) else {
        return Vec::new();
    };
    let mut pairs = Vec::new();
    for method in ["get", "post", "put", "delete", "patch"] {
        let needle = format!("@app.{method}(");
        let mut from = 0usize;
        while let Some(offset) = text[from..].find(&needle) {
            let at = from + offset + needle.len();
            from = at;
            let Some(path) = first_literal(&text[at..]) else {
                continue;
            };
            pairs.push((method.to_string(), path));
        }
    }
    pairs.dedup();
    pairs
}

/// Does this entry forward to the engine rather than to its own process?
fn forwards_to_engine(entry: &Entry) -> bool {
    entry.url.contains("8099")
}

/// Every address a retired service published, checked against the form its gateway entry forwards.
pub fn audit(root: &Path) -> Vec<String> {
    let engine = engine_routes(root);
    let mut findings = Vec::new();
    for entry in entries(root) {
        if !forwards_to_engine(&entry) {
            continue;
        }
        if root.join(&entry.name).is_dir() {
            continue; // still on disk, so its own app answers
        }
        let published = published(root, &entry.name);
        if published.is_empty() {
            continue;
        }
        let keeps = !entry.strip_prefix;
        let prefix = entry.path.trim_end_matches('/');
        for (_, path) in published {
            if is_skipped(&path) {
                continue;
            }
            let address = if keeps {
                format!("{prefix}{path}")
            } else {
                path.clone()
            };
            if !engine.contains(&shape(&address)) {
                findings.push(format!(
                    "{}: {address} reaches nothing in the engine",
                    entry.name
                ));
            }
        }
    }
    findings
}

/// Published method/path pairs the engine does not answer.
pub fn method_gaps(root: &Path) -> Vec<String> {
    let engine = engine_method_routes(root);
    let mut findings = Vec::new();
    for entry in entries(root) {
        if root.join(&entry.name).is_dir() {
            continue;
        }
        let prefix = entry.path.trim_end_matches('/');
        for (method, path) in published(root, &entry.name) {
            if is_skipped(&path) {
                continue;
            }
            let prefixed = shape(&format!("{prefix}{path}"));
            let flat = shape(&path);
            let answered = [(method.clone(), prefixed), (method.clone(), flat)]
                .iter()
                .any(|pair| engine.contains(pair));
            if !answered {
                findings.push(format!(
                    "{}: {} {path} reaches nothing in the engine",
                    entry.name,
                    method.to_uppercase()
                ));
            }
        }
    }
    findings
}

/// Paths two engine-served services both claim once their gateway prefix is stripped.
///
/// With one engine holding every route, a stripped path is a single address: if two services both publish `/stats` and
/// both are stripped, one of them silently reads the other's handler. Requests still answer, which is why this needs
/// checking rather than trusting a green run.
pub fn stripped_collisions(root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entry in entries(root) {
        if !entry.strip_prefix || root.join(&entry.name).is_dir() {
            continue;
        }
        for (_, path) in published(root, &entry.name) {
            if is_skipped(&path) {
                continue;
            }
            owners
                .entry(shape(&path))
                .or_default()
                .insert(entry.name.clone());
        }
    }
    owners
        .into_iter()
        .filter(|(_, names)| names.len() > 1)
        .map(|(path, names)| (path, names.into_iter().collect()))
        .collect()
}

/// The root the binary and the tests share.
pub fn root() -> std::path::PathBuf {
    repo_root()
}
