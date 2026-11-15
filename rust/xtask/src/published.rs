#![forbid(unsafe_code)]
//! What each routed service answers, read from the engine instead of from a service that no longer exists.
//!
//! The catalogue used to read each service's `main.py` to list its endpoints. Every one of those services is gone, so the
//! list came out empty for all 137 of them - a catalogue that says what the gateway routes and then declines to say what
//! answers. The addresses did not disappear with the Python: the engine answers every one of them, at the same published
//! paths, which is why no client changed. So the list is derived from the engine's own routes, attributed to the service
//! whose gateway prefix contains the address.
//!
//! One consequence is worth stating because it looks like a loss: nothing here can tell you which *service* once wrote a
//! line of Python. The prefix is the ownership boundary now, and after the retirement the code that answers an address is
//! chosen by the prefix, not by the file it used to live in.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::gateway;

/// One address a service publishes: the method and the path a client calls.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Address {
    pub method: String,
    pub path: String,
}

impl Address {
    /// The address as it is written down: `GET /accounting/accounts`.
    pub fn written(&self) -> String {
        format!("{} {}", self.method, self.path)
    }
}

/// Every address the engine answers, sorted, with parameter names blanked so `{product_id}` and `{}` are one shape.
pub fn addresses(root: &Path) -> Vec<Address> {
    let mut found: Vec<Address> = gateway::engine_method_routes(root)
        .into_iter()
        .map(|(method, path)| Address {
            method: method.to_ascii_uppercase(),
            path,
        })
        .collect();
    found.sort();
    found
}

/// The addresses each routed service publishes to a client, keyed by service name.
///
/// Two sources, because neither is complete on its own. The address a client calls is the gateway prefix plus the path the
/// service published, and for a service the gateway forwards with its prefix stripped, the engine answers the bare path -
/// so the engine alone cannot say which prefix that bare path belongs to. Those arrive from the last version of the
/// service's own `main.py`, read out of git. The engine supplies the rest: a door added in Rust that the old service never
/// published is answered under the prefix and belongs to whoever owns the prefix.
pub fn per_service(root: &Path) -> BTreeMap<String, Vec<Address>> {
    let rows = crate::catalog::rows_without_endpoints(root);
    let prefixes: Vec<String> = rows.iter().map(|row| row.gateway_path.clone()).collect();
    let under = by_prefix(root, &prefixes);
    let answered = addresses(root);
    let keeping = keeping(root);
    let mut published: BTreeMap<String, Vec<Address>> = BTreeMap::new();
    for row in &rows {
        let prefix = row.gateway_path.trim_end_matches('/').to_string();
        let mut found: BTreeSet<Address> = BTreeSet::new();
        for (method, path) in gateway::published(root, &row.name) {
            let method = method.to_ascii_uppercase();
            // What the engine is asked for is not always what the client writes: the gateway keeps the prefix for most
            // entries and strips it for the rest, so the engine's side of the same address is the bare path there. Asking
            // the engine for the client's form would report every stripping door as unanswered, which is how 26 of them
            // looked dead when they were not.
            let kept = keeping.get(&row.name).copied().unwrap_or(true);
            let asked = if kept {
                format!("{prefix}{path}")
            } else {
                path.clone()
            };
            // ... and it is kept only in a shape something answers. A path ending in a slash is the habitual one -
            // `/accounts/` - and the engine answers `/accounts`, so the address is published as the shape that answers.
            // One nothing answers is not published at all: a snapshot that names it is a claim the engine refuses.
            let Some(answered_path) = answered_shape(&answered, &method, &asked) else {
                continue;
            };
            let written = if kept {
                answered_path
            } else {
                format!("{prefix}{answered_path}")
            };
            found.insert(Address {
                method,
                path: written,
            });
        }
        for address in under.get(&row.gateway_path).cloned().unwrap_or_default() {
            found.insert(address);
        }
        published.insert(row.name.clone(), found.into_iter().collect());
    }
    published
}

/// Which entries keep the gateway prefix when forwarding, from the manifest rather than guessed.
fn keeping(root: &Path) -> BTreeMap<String, bool> {
    gateway::entries(root)
        .into_iter()
        .map(|entry| (entry.name.clone(), !entry.strip_prefix))
        .collect()
}

/// The shape the engine answers for a path, or `None` when nothing answers it.
///
/// `gateway::shape` blanks parameter names *and* trims a trailing slash, which is right for comparing routes and wrong for
/// this: the trailing slash is the difference between an address that answers and one that 404s, so it is tried first
/// before the trimmed form is offered.
fn answered_shape(answered: &[Address], method: &str, asked: &str) -> Option<String> {
    let shaped = gateway::shape(asked);
    let mut candidates = vec![shaped.clone()];
    if asked.len() > 1 && asked.ends_with('/') {
        candidates.insert(0, format!("{shaped}/"));
    }
    candidates
        .into_iter()
        .find(|candidate| answers(answered, method, candidate))
}

/// Does the engine route `engine` answer the path `path`, treating one parameter segment as any one segment?
fn same_shape(engine: &str, path: &str) -> bool {
    let engine: Vec<&str> = engine.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    engine.len() == path.len()
        && engine.iter().zip(&path).all(|(pattern, segment)| {
            // a parameter stands for a real segment, never an empty one: `/accounts/` does not answer where
            // `/accounts/{}` is routed, and that is exactly the difference a trailing slash makes
            if *pattern == "{}" {
                !segment.is_empty()
            } else {
                pattern == segment
            }
        })
}

/// Does the engine answer this address? A parameter stands for a real segment, so a trailing slash is never a parameter.
pub fn answers(answered: &[Address], method: &str, path: &str) -> bool {
    answered.iter().any(|address| {
        // the scanner reads the method name as it was written; the snapshot writes it in capitals
        address.method.eq_ignore_ascii_case(method) && same_shape(&address.path, path)
    })
}

/// The addresses the snapshot publishes that the engine does not answer.
///
/// A published address is a promise: a client reads the snapshot, calls the address and expects an answer. The snapshot used
/// to repeat the retired services' own paths, and 903 of them answered 404 - `/accounting/accounts/` among them, which is
/// how the app's accounting screens came back empty. This holds every published address to the one test that matters.
pub fn unanswered(root: &Path) -> Vec<String> {
    let answered = addresses(root);
    let keeping = keeping(root);
    let prefixes: BTreeMap<String, String> = crate::catalog::rows_without_endpoints(root)
        .into_iter()
        .map(|row| (row.name, row.gateway_path.trim_end_matches('/').to_string()))
        .collect();
    let mut problems = Vec::new();
    for (service, addresses) in per_service(root) {
        let kept = keeping.get(&service).copied().unwrap_or(true);
        let prefix = prefixes.get(&service).cloned().unwrap_or_default();
        for address in addresses {
            let mut asked = vec![address.path.clone()];
            if !kept {
                if let Some(rest) = address.path.strip_prefix(&prefix) {
                    if !rest.is_empty() && rest != "/" {
                        asked.push(rest.to_string());
                    }
                }
            }
            // one answered form is enough: the client writes the prefixed address and the gateway strips it, so the
            // engine only ever sees the bare one
            let unanswered = asked
                .into_iter()
                .all(|candidate| answered_shape(&answered, &address.method, &candidate).is_none());
            if unanswered {
                problems.push(format!("{service} publishes {}", address.written()));
            }
        }
    }
    problems
}

/// The addresses under each gateway prefix, in the order a reader expects: by path, then by method.
///
/// A prefix claims an address when the address *is* the prefix or continues it at a segment boundary, so `/accounting`
/// does not claim `/accounting-standards/standards`.
pub fn by_prefix(root: &Path, prefixes: &[String]) -> BTreeMap<String, Vec<Address>> {
    let all = addresses(root);
    let mut under: BTreeMap<String, Vec<Address>> = BTreeMap::new();
    for prefix in prefixes {
        let trimmed = prefix.trim_end_matches('/');
        let owned: Vec<Address> = all
            .iter()
            .filter(|address| {
                address.path == trimmed
                    || address
                        .path
                        .strip_prefix(trimmed)
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .cloned()
            .collect();
        under.insert(prefix.clone(), owned);
    }
    under
}

/// The prefixes that claim an address belonging to another prefix as well, which would make ownership ambiguous.
pub fn overlaps(root: &Path, prefixes: &[String]) -> Vec<String> {
    let under = by_prefix(root, prefixes);
    let seen: BTreeSet<(String, String)> = under
        .iter()
        .flat_map(|(prefix, addresses)| {
            addresses
                .iter()
                .map(move |address| (address.written(), prefix.clone()))
        })
        .collect();
    let mut counts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (written, prefix) in seen {
        counts.entry(written).or_default().insert(prefix);
    }
    counts
        .into_iter()
        .filter(|(_, prefixes)| prefixes.len() > 1)
        .map(|(written, prefixes)| {
            format!(
                "{written} is claimed by {}",
                prefixes.into_iter().collect::<Vec<_>>().join(" and ")
            )
        })
        .collect()
}

/// The published address set as the document a reader diffs after a change.
pub fn snapshot(root: &Path) -> String {
    let rows = crate::catalog::rows_without_endpoints(root);
    let published = per_service(root);
    let mut services = Vec::new();
    for row in &rows {
        let addresses = published.get(&row.name).cloned().unwrap_or_default();
        let endpoints: Vec<String> = addresses.iter().map(Address::written).collect();
        services.push(serde_json::json!({
            "name": row.name,
            "port": row.port,
            "path": row.gateway_path,
            "endpoints": endpoints,
        }));
    }
    let document = serde_json::json!({ "count": services.len(), "services": services });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&document).expect("a document of plain JSON values")
    )
}

/// Where the snapshot lives.
pub fn snapshot_path(root: &Path) -> std::path::PathBuf {
    root.join("docs").join("routes-snapshot.json")
}

/// Write the snapshot, or report whether the committed one is current.
pub fn write(root: &Path, check: bool) -> Result<(), String> {
    let path = snapshot_path(root);
    let generated = snapshot(root);
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    if check {
        if committed == generated {
            return Ok(());
        }
        return Err(format!(
            "{} is not what the engine publishes; run `cargo run -p xtask -- routes`",
            path.display()
        ));
    }
    crate::repository::write_atomic(&path, &generated)
        .map_err(|error| format!("{}: {error}", path.display()))
}
