//! The catalogue: every service the gateway routes, who owns it, and where its arithmetic runs.
//!
//! Two inventories already existed and neither was enough alone. `gateway/go/config/services.json` says what the gateway
//! actually serves, in the order it serves it; `catalog/ownership.json` says which domain owns each service and where
//! that domain's code lands. The catalogue puts them in one place, per service, so a reader - or a test - does not have
//! to join two files to answer "who owns this and is it on Rust yet".
//!
//! It is generated for the same reason the ownership table is data: a hand-written catalogue is a second inventory, and
//! a catalogue that can disagree with the router is worse than none.
#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// What one routed service looks like once the two sources are joined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub domain: String,
    pub target: String,
    pub port: Option<i64>,
    pub gateway_path: String,
    pub auth_required: bool,
    pub engine: String,
    pub records: bool,
    pub endpoints: Vec<String>,
}

/// The repository root, found from this crate's manifest directory (`rust/xtask`).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("rust/xtask lives two levels under the root")
        .to_path_buf()
}

fn read(root: &Path, relative: &str) -> String {
    let path = root.join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn json(root: &Path, relative: &str) -> Value {
    let text = read(root, relative);
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{relative}: {error}"))
}

/// The services the gateway routes, in its own order.
fn routed(root: &Path) -> Vec<Value> {
    json(root, "gateway/go/config/services.json")["services"]
        .as_array()
        .expect("services.json carries a services array")
        .clone()
}

/// The domains, and who owns each service.
fn ownership(root: &Path) -> Value {
    json(root, "catalog/ownership.json")
}

/// The services the port tracker lists as wired to the Rust library.
///
/// Only the bullets under "Wired to the Rust library" count. The tracker also lists services whose maths the library
/// already has but which are not served from it yet, and searching the whole file for a name counted those as ported -
/// and counted `currency-service` because `multi-currency-service` contains it. A bullet may name several services that
/// now share one crate, so the whole list before the arrow is read: "a-service, b-service and c-service -> crate".
fn ported_names(root: &Path) -> Vec<String> {
    let text = read(root, "rust/CALCULATOR-PORT.md");
    let mut listed: Vec<String> = Vec::new();
    let mut in_section = false;
    for line in text.lines() {
        let stripped = line.trim();
        if let Some(heading) = stripped.strip_prefix("## ") {
            in_section = heading.contains("Wired to the Rust library");
            continue;
        }
        if !in_section {
            continue;
        }
        let Some(rest) = stripped
            .strip_prefix("- ")
            .or_else(|| stripped.strip_prefix("-   "))
        else {
            continue;
        };
        // The names run until the arrow or the parenthetical that says where the crate is.
        let names_part = match rest.find("->") {
            Some(at) => &rest[..at],
            None => match rest.find('(') {
                Some(at) => &rest[..at],
                None => rest,
            },
        };
        for name in names_part.split(',') {
            for piece in name.split(" and ") {
                let name = piece.trim().trim_end_matches(':');
                if !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
                {
                    listed.push(name.to_string());
                }
            }
        }
    }
    let owners = ownership(root)["services"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    listed
        .into_iter()
        .filter(|name| owners.contains_key(name))
        .collect()
}

/// The endpoints a service still declares itself. Every service is Rust now, so this is empty; it stays because a
/// catalogue row should say how a service is reached, and the day a service keeps its own app this reads it.
fn endpoints_of(root: &Path, name: &str) -> Vec<String> {
    let path = root.join(name).join("main.py");
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    // In the order the service declares them, which is how a reader expects to find them: grouping by method put every
    // GET above every POST and made the list look like a different service.
    let mut found: Vec<String> = Vec::new();
    let mut at = 0;
    while let Some(offset) = text[at..].find("@app.") {
        let start = at + offset + "@app.".len();
        let rest = &text[start..];
        let Some(open) = rest.find('(') else { break };
        let method = rest[..open].to_ascii_uppercase();
        let tail = &rest[open + 1..];
        let Some(quote) = tail.find('"') else { break };
        if let Some(close) = tail[quote + 1..].find('"') {
            found.push(format!("{method} {}", &tail[quote + 1..quote + 1 + close]));
        }
        at = start;
    }
    found
}

/// One row per routed service, in the order the gateway lists them.
pub fn rows(root: &Path) -> Vec<Row> {
    let owners = ownership(root);
    let domain_of = owners["services"].as_object().cloned().unwrap_or_default();
    let targets: Vec<(String, String)> = owners["domains"]
        .as_array()
        .map(|domains| {
            domains
                .iter()
                .map(|domain| {
                    (
                        domain["name"].as_str().unwrap_or_default().to_string(),
                        domain["target"].as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let ported = ported_names(root);

    routed(root)
        .iter()
        .map(|service| {
            let name = service["name"].as_str().unwrap_or_default().to_string();
            let domain = domain_of
                .get(&name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let target = targets
                .iter()
                .find(|(owner, _)| *owner == domain)
                .map(|(_, target)| target.clone())
                .unwrap_or_default();
            Row {
                records: false,
                endpoints: endpoints_of(root, &name),
                engine: if ported.contains(&name) {
                    "rust".to_string()
                } else {
                    "python".to_string()
                },
                port: service["port"].as_i64(),
                gateway_path: service["path"].as_str().unwrap_or_default().to_string(),
                auth_required: service["auth_required"].as_bool().unwrap_or(false),
                name,
                domain,
                target,
            }
        })
        .collect()
}

/// A YAML scalar, quoted only when it would otherwise mean something else.
fn scalar(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || "/._-".contains(c))
        && !text.chars().last().is_some_and(char::is_whitespace)
        && !text.contains(": ")
        && !text.contains(" #")
        && !text.ends_with(':')
        && !matches!(
            text,
            "true" | "false" | "null" | "yes" | "no" | "on" | "off"
        );
    if plain {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', "''"))
    }
}

fn domain_block(out: &mut String, name: &str, responsibility: &str, target: &str, services: usize) {
    out.push_str("domain:\n");
    let _ = writeln!(out, "  name: {}", scalar(name));
    let _ = writeln!(out, "  responsibility: {}", scalar(responsibility));
    let _ = writeln!(out, "  target: {}", scalar(target));
    let _ = writeln!(out, "  services: {services}");
}

fn service_block(out: &mut String, row: &Row) {
    let _ = writeln!(out, "- name: {}", scalar(&row.name));
    let _ = writeln!(out, "  domain: {}", scalar(&row.domain));
    let _ = writeln!(out, "  target: {}", scalar(&row.target));
    match row.port {
        Some(port) => {
            let _ = writeln!(out, "  port: {port}");
        }
        None => out.push_str("  port: null\n"),
    }
    let _ = writeln!(out, "  gateway_path: {}", scalar(&row.gateway_path));
    let _ = writeln!(out, "  auth_required: {}", row.auth_required);
    let _ = writeln!(out, "  engine: {}", scalar(&row.engine));
    let _ = writeln!(out, "  records: {}", row.records);
    if row.endpoints.is_empty() {
        out.push_str("  endpoints: []\n");
    } else {
        out.push_str("  endpoints:\n");
        for endpoint in &row.endpoints {
            let _ = writeln!(out, "  - {}", scalar(endpoint));
        }
    }
}

/// The whole estate in one file: every routed service, then the domains that own them.
pub fn catalog_text(root: &Path) -> String {
    let rows = rows(root);
    let owners = ownership(root);
    let mut out = String::from("# Generated by rust/xtask. Do not edit by hand.\n");
    out.push_str("catalog:\n  version: 1\n  generated_by: rust/xtask\n");
    out.push_str("  authority: gateway/go/config/services.json for routes, catalog/ownership.json for ownership\n");
    let _ = writeln!(out, "  services: {}", rows.len());
    out.push_str("domains:\n");
    for domain in owners["domains"].as_array().cloned().unwrap_or_default() {
        let _ = writeln!(
            out,
            "- name: {}",
            scalar(domain["name"].as_str().unwrap_or_default())
        );
        let _ = writeln!(
            out,
            "  responsibility: {}",
            scalar(domain["responsibility"].as_str().unwrap_or_default())
        );
        let _ = writeln!(
            out,
            "  target: {}",
            scalar(domain["target"].as_str().unwrap_or_default())
        );
    }
    out.push_str("services:\n");
    for row in &rows {
        service_block(&mut out, row);
    }
    out
}

/// One file per domain, so a reader who cares about treasury does not read all 138 rows.
pub fn domain_texts(root: &Path) -> Vec<(String, String)> {
    let rows = rows(root);
    let owners = ownership(root);
    let domains = owners["domains"].as_array().cloned().unwrap_or_default();
    let mut names: Vec<String> = rows
        .iter()
        .map(|row| {
            if row.domain.is_empty() {
                "unassigned".to_string()
            } else {
                row.domain.clone()
            }
        })
        .collect();
    names.sort();
    names.dedup();

    names
        .into_iter()
        .map(|name| {
            let known = domains
                .iter()
                .find(|domain| domain["name"].as_str() == Some(name.as_str()));
            let responsibility = known
                .and_then(|domain| domain["responsibility"].as_str())
                .unwrap_or("unowned");
            let target = known
                .and_then(|domain| domain["target"].as_str())
                .unwrap_or_default();
            let mine: Vec<&Row> = rows
                .iter()
                .filter(|row| row.domain == name || (row.domain.is_empty() && name == "unassigned"))
                .collect();
            let mut out = String::from("# Generated by rust/xtask. Do not edit by hand.\n");
            domain_block(&mut out, &name, responsibility, target, mine.len());
            out.push_str("services:\n");
            for row in mine {
                service_block(&mut out, row);
            }
            (name, out)
        })
        .collect()
}

/// The catalogue's own README, written next to the files so the two cannot describe different things.
pub fn readme_text() -> String {
    String::from(
        "# Catalogue\n\n\
The one place the estate is described: every service the gateway routes, the domain that owns it, where that domain's code\n\
lives, and whether its figures already come from Rust.\n\n\
Generated by `cargo run -p xtask -- catalog` from two sources of truth that already existed:\n\n\
1. `gateway/go/config/services.json` - the routes the gateway actually serves, in its order.\n\
2. `catalog/ownership.json` - which domain owns each service, and where that domain's code lands.\n\n\
Nothing here is typed by hand, and `rust/xtask/tests/catalog_is_current.rs` fails if these files drift from the generator.\n\
That matters more than it sounds: this repository has already had three different counts of the same services, and a\n\
catalogue that can disagree with the router is worse than none.\n\n\
## Files\n\n\
```\n\
catalog/services/catalog.yaml      every service, in gateway order\n\
catalog/services/<domain>.yaml     the same services grouped by owner\n\
```\n\n\
Each entry carries the port, the gateway path, whether authentication is required, how many endpoints the service answers\n\
itself, whether it still owns records, and whether its figures come from the Rust engine. The `target` field is where the\n\
domain lands in the Rust workspace.\n\n\
## What this is not\n\n\
It is not an API contract. The contracts under `contracts/` are the versioned, agreed shape of what a caller may ask for.\n\n\
It is not a deployment plan either. `engine` says where the arithmetic runs, not what should be deployed separately.\n",
    )
}

/// Write the catalogue, or with `check` only report whether the committed copy is current.
pub fn write(root: &Path, check: bool) -> Result<(), String> {
    let mut expected: Vec<(PathBuf, String)> = vec![
        (
            root.join("catalog/services/catalog.yaml"),
            catalog_text(root),
        ),
        (root.join("catalog/README.md"), readme_text()),
    ];
    for (domain, text) in domain_texts(root) {
        expected.push((
            root.join("catalog/services").join(format!("{domain}.yaml")),
            text,
        ));
    }

    // A domain that no longer exists would leave its file behind, describing services it no longer owns.
    let directory = root.join("catalog/services");
    if let Ok(entries) = std::fs::read_dir(&directory) {
        let wanted: Vec<PathBuf> = expected.iter().map(|(path, _)| path.clone()).collect();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "yaml") && !wanted.contains(&path) {
                if check {
                    return Err(format!(
                        "{} is stale: no domain owns it any more",
                        path.display()
                    ));
                }
                std::fs::remove_file(&path)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
            }
        }
    }

    let mut stale = Vec::new();
    for (path, text) in expected {
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current == text {
            continue;
        }
        if check {
            stale.push(path.display().to_string());
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        std::fs::write(&path, &text).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    if stale.is_empty() {
        Ok(())
    } else {
        stale.sort();
        Err(format!(
            "the catalogue is out of date, run `cargo run -p xtask -- catalog`: {}",
            stale.join(", ")
        ))
    }
}
