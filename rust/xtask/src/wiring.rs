//! Nothing is wired twice, and nothing is written twice.
//!
//! Duplication in this repository has not looked like copy-paste. It has looked like two things answering to one name
//! and the second silently winning: a calculator registered under a key that already existed, so the first
//! registration was dead code that looked alive; two gateway entries claiming one path, so one service's callers were
//! answered by another's; one source file in two crates, waiting to drift.
//!
//! Each rule here is a shape that passed every behavioural test and still cost something.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// A file below this size is a re-export module and is expected to look like another.
const TRIVIAL: usize = 400;

fn rust_sources(directory: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            rust_sources(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            into.push(path);
        }
    }
}

/// Calculator names registered more than once in the engine, where the earlier registration is dead code.
pub fn duplicate_calculator_registrations(root: &Path) -> Vec<String> {
    let directory = root.join("rust/apps/api-server/src");
    let mut files = Vec::new();
    rust_sources(&directory, &mut files);
    files.sort();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        let mut from = 0usize;
        while let Some(offset) = text[from..].find("map.insert(") {
            let at = from + offset + "map.insert(".len();
            from = at;
            let rest = text[at..].trim_start();
            let Some(rest) = rest.strip_prefix('"') else {
                continue;
            };
            let Some(end) = rest.find('"') else {
                continue;
            };
            let name = &rest[..end];
            if name.is_empty() {
                continue;
            }
            *counts.entry(name.to_string()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(name, count)| format!("{name} registered {count} times"))
        .collect()
}

fn gateway(root: &Path) -> Value {
    let path = root.join("gateway/go/config/services.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|problem| panic!("{} is unreadable: {problem}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|problem| panic!("{} is not JSON: {problem}", path.display()))
}

/// Two services answering one gateway path.
pub fn duplicate_gateway_paths(root: &Path) -> Vec<String> {
    let document = gateway(root);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for entry in document
        .get("services")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
    {
        if let Some(path) = entry.get("path").and_then(Value::as_str) {
            *counts.entry(path.to_string()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(path, count)| format!("{path} claimed by {count} services"))
        .collect()
}

fn collect_names(node: &Value, into: &mut Vec<String>) {
    match node {
        Value::Object(map) => {
            for (key, value) in map {
                if matches!(key.as_str(), "name" | "service" | "target") {
                    if let Some(text) = value.as_str() {
                        into.push(text.to_string());
                    }
                }
                collect_names(value, into);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_names(item, into);
            }
        }
        _ => {}
    }
}

/// One service named twice in the route table.
pub fn duplicate_service_names(root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    collect_names(&gateway(root), &mut names);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for name in names {
        *counts.entry(name).or_default() += 1;
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(name, count)| format!("{name} appears {count} times"))
        .collect()
}

/// Two source files in the workspace with the same bytes, one of which is a copy waiting to drift.
pub fn identical_source_files(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    rust_sources(&root.join("rust/crates"), &mut files);
    files.sort();
    let mut seen: BTreeMap<Vec<u8>, PathBuf> = BTreeMap::new();
    let mut copies = Vec::new();
    for file in files {
        let Ok(body) = std::fs::read(&file) else {
            continue;
        };
        if body.len() < TRIVIAL {
            continue;
        }
        if let Some(first) = seen.get(&body) {
            copies.push(format!(
                "{} = {}",
                first.strip_prefix(root).unwrap_or(first).display(),
                file.strip_prefix(root).unwrap_or(&file).display()
            ));
        } else {
            seen.insert(body, file);
        }
    }
    copies
}

/// Services de-registered as duplicates that have crept back into the router or the cluster.
///
/// The lists behind this live in `catalog/deregistered-duplicates.json` because they are data about the estate, not
/// code: 179 services were removed as clones of a survivor or as calculators the Rust library answers, and the point of
/// writing them down is that their return is a decision rather than an accident.
pub fn resurrected_duplicates(root: &Path) -> Vec<String> {
    let path = root.join("catalog/deregistered-duplicates.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return vec![format!(
            "{} is missing, so nothing is pinned",
            path.display()
        )];
    };
    let document: Value = serde_json::from_str(&text)
        .unwrap_or_else(|problem| panic!("{} is not JSON: {problem}", path.display()));
    let names: Vec<String> = document
        .get("services")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut findings = Vec::new();
    if names.len() < 100 {
        findings.push(format!(
            "the pinned list holds {} names, which cannot be the families that were removed",
            names.len()
        ));
    }
    let routed: Vec<String> = gateway(root)
        .get("services")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("name").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    for name in &names {
        if routed.contains(name) {
            findings.push(format!("{name} is routed again"));
        }
    }
    // The cluster side of this rule reads the whole manifest tree rather than one generated file, because the generated
    // file was deleted and a gate that reads a file nothing writes is a gate that passes over nothing.
    for (manifest, text) in crate::manifests::all(root) {
        for name in &names {
            if text.contains(&format!("name: {name}")) {
                findings.push(format!("{name} is deployed again ({manifest})"));
            }
        }
    }
    findings
}
