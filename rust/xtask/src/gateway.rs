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
    /// The port the entry states, kept only so the manifest can be checked against its own addresses.
    pub port: Option<i64>,
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
            port: entry.get("port").and_then(Value::as_i64),
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
pub(crate) fn engine_sources(root: &Path) -> Vec<String> {
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
pub(crate) fn mounts_for(text: &str) -> Vec<String> {
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
pub(crate) fn call_arguments(text: &str, open_at: usize) -> Option<&str> {
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
pub(crate) fn first_literal(arguments: &str) -> Option<String> {
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

/// Whether this checkout is shallow.
///
/// It matters because part of what the catalogue reports is read out of history: an address a retired service published is
/// taken from its last `main.py`, which lives in the commit before the deletion. A shallow clone has neither that commit nor
/// its parent, so every one of those addresses silently comes back empty and the catalogue is written wrong rather than
/// failing. A caller that derives anything from history asks this first.
pub fn shallow(root: &Path) -> bool {
    git(root, &["rev-parse", "--is-shallow-repository"])
        .map(|answer| answer.trim() == "true")
        .unwrap_or(false)
}

/// The reason a shallow checkout cannot be read for history, in the words of the fix.
pub fn needs_full_history(root: &Path) -> Option<String> {
    shallow(root).then(|| {
        "this checkout is shallow, so the last published addresses of the retired services cannot be read: the catalogue is\
         \nderived from them. Check out with full history (actions/checkout with fetch-depth: 0) and try again."
            .to_string()
    })
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

/// Entries the engine answers, whose manifest still forwards them to a port nothing listens on.
///
/// The gateway's own manifest names the destination, and after the retirement the only destination left is the engine. When
/// a service was retired its entry kept the address a client calls - which is the point, no client changed - but fifteen of
/// them kept their old container port as well, so the prefix was answered by the engine and forwarded to a dead socket. A
/// caller saw a 502 from a door the manifest says is open.
pub fn misrouted(root: &Path) -> Vec<String> {
    let answered = engine_routes(root);
    let mut findings = Vec::new();
    for entry in entries(root) {
        let prefix = entry.path.trim_end_matches('/');
        let reaches_engine = answered
            .iter()
            .any(|path| path == prefix || path.starts_with(&format!("{prefix}/")));
        if reaches_engine && !forward_to_engine(&entry) {
            findings.push(format!(
                "{}: {} is answered by the engine but the manifest forwards it to {}",
                entry.name, entry.path, entry.url
            ));
        }
    }
    findings
}

/// The routes the gateway declares in Go rather than in the catalogue, and whether the engine answers them.
///
/// The gateway carried twenty-two of these beside `services.json`: `/accounts`, `/journal-entries`, `/budgets`, `/customers`,
/// `/banks`, `/customers` and their neighbours, each with its own `*_SERVICE_URL` in the manifest, each pointing at a service
/// retired into the engine. Nothing read the catalogue for those prefixes and nothing read the code for the catalogue's, so a
/// client calling `/accounts` reached a host that does not resolve while `/accounting/accounts` reached the engine that
/// answers it. Neither the address audit, the parity tests nor the manifest gate could see it: they read the manifest, and the
/// manifest was not where those routes were.
///
/// One static route is legitimate: `/identity`, because signing in has to answer without a token. So the rule is not "no static
/// routes" but "a static route is an address the engine answers", which is what makes it a door rather than a promise.
pub fn static_routes(root: &Path) -> Vec<(String, bool)> {
    let Ok(text) = std::fs::read_to_string(root.join("gateway/go/config/config.go")) else {
        return Vec::new();
    };
    let answered = engine_routes(root);
    let mut routes = Vec::new();
    for line in text.lines() {
        let Some(at) = line.find("{Path: \"") else {
            continue;
        };
        let rest = &line[at + "{Path: \"".len()..];
        let Some(end) = rest.find('"') else {
            continue;
        };
        let path = &rest[..end];
        let prefix = path.trim_end_matches('/');
        let reaches_engine = answered
            .iter()
            .any(|address| address == prefix || address.starts_with(&format!("{prefix}/")));
        routes.push((path.to_string(), reaches_engine));
    }
    routes
}

/// Static routes the engine does not answer: a door to a service that is not there, invisible to every other check.
pub fn dead_static_routes(root: &Path) -> Vec<String> {
    let routes = static_routes(root);
    if routes.is_empty() {
        return vec![
            "the gateway declares no route of its own, so this scan is reading nothing (the /identity door has to be there)"
                .to_string(),
        ];
    }
    routes
        .into_iter()
        .filter(|(_, reaches_engine)| !reaches_engine)
        .map(|(path, _)| {
            format!("{path} is a route the gateway declares itself, and the engine answers nothing under it")
        })
        .collect()
}

/// Does this entry forward to the engine rather than to a container of its own?
pub(crate) fn forward_to_engine(entry: &Entry) -> bool {
    forwards_to_engine(entry)
}

/// Entries whose stated port is not the port their address names.
///
/// The manifest carries both, and only the url is ever dialled, so the port was free to drift from it while still being read
/// as fact by anyone opening the file. Fourteen entries had drifted: they forwarded to the engine on 8099 and still said they
/// listened on the container port their retired service used. This keeps the two agreeing, which is the least a manifest owes
/// the person reading it.
pub fn port_drift(root: &Path) -> Vec<String> {
    let mut findings = Vec::new();
    for entry in entries(root) {
        let Some(stated) = entry.port else {
            continue;
        };
        let Some(named) = entry
            .url
            .rsplit(':')
            .next()
            .and_then(|end| end.parse::<i64>().ok())
        else {
            continue;
        };
        if stated != named {
            findings.push(format!(
                "{}: says port {stated} and forwards to {}",
                entry.name, entry.url
            ));
        }
    }
    findings
}
