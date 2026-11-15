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
    let mut published: BTreeMap<String, Vec<Address>> = BTreeMap::new();
    for row in &rows {
        let prefix = row.gateway_path.trim_end_matches('/').to_string();
        let mut found: BTreeSet<Address> = BTreeSet::new();
        for (method, path) in gateway::published(root, &row.name) {
            found.insert(Address {
                method: method.to_ascii_uppercase(),
                path: format!("{prefix}{path}"),
            });
        }
        for address in under.get(&row.gateway_path).cloned().unwrap_or_default() {
            found.insert(address);
        }
        published.insert(row.name.clone(), found.into_iter().collect());
    }
    published
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
    std::fs::write(&path, generated).map_err(|error| format!("{}: {error}", path.display()))
}
