//! The gateway, the cluster and the code have to describe the same estate.
//!
//! Three retirement batches found services wired into the cluster after they had left the gateway manifest, and nothing
//! was checking, so a retired service could keep a live deployment behind it. A second defect lived in the same place:
//! a Deployment whose liveness probe hit `/` on an app with no root handler, which the cluster answers with a 404, so
//! the pod is killed and restarted forever and never becomes Ready. A green test suite and a valid manifest add up to a
//! service that cannot run.
//!
//! The manifests are read as text rather than parsed. The workspace carries no YAML parser on purpose - it adds a
//! dependency to a tool that has to build on a machine with no network - so the two things worth knowing are read by
//! their line shape: a document's `name:` and its probes' `path:`. A scanner that finds nothing fails the gate instead
//! of passing over an empty set.
#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::path::Path;

use crate::gateway;

/// Deployment names that are infrastructure rather than a service the gateway routes.
const INFRASTRUCTURE: [&str; 8] = [
    "api-gateway",
    "calculation-api",
    "neo4j",
    "neo4j-data",
    "redis",
    "vimbai-config",
    "vimbai-sa",
    "vimbai-secrets",
];

fn manifests(root: &Path) -> Vec<(String, String)> {
    let directory = root.join("infrastructure/kubernetes");
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut found: Vec<(String, String)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml" || extension == "yml")
        })
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&path).ok()?;
            Some((name, text))
        })
        .collect();
    found.sort();
    found
}

fn routed_names(root: &Path) -> BTreeSet<String> {
    gateway::entries(root)
        .into_iter()
        .map(|entry| entry.name)
        .collect()
}

/// The names the cluster deploys that look like a service rather than infrastructure.
fn deployed_services(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for line in text.lines() {
        let trimmed = line.trim_start_matches(char::is_whitespace);
        let Some(rest) = trimmed.strip_prefix("name: ") else {
            continue;
        };
        let name = rest.trim().trim_matches('"').to_string();
        if name.ends_with("-service") && !INFRASTRUCTURE.contains(&name.as_str()) {
            names.insert(name);
        }
    }
    names
}

/// Services the cluster deploys that the gateway does not route: a live pod behind a retired address.
///
/// The estate is one engine now, so the passing state is that no per-service Deployment exists at all. That is exactly
/// why the scan asserts it saw Deployments: "found no service pods" and "read no manifests" are the same empty set, and
/// only one of them is good news.
pub fn deployed_but_unrouted(root: &Path) -> Vec<String> {
    let routed = routed_names(root);
    let mut findings = BTreeSet::new();
    let mut deployments = 0usize;
    for (name, text) in manifests(root) {
        for line in text.lines() {
            let trimmed = line.trim_start_matches(char::is_whitespace);
            if trimmed == "kind: Deployment" || trimmed == "kind: Deployment ".trim() {
                deployments += 1;
            }
        }
        for deployed in deployed_services(&text) {
            if !routed.contains(&deployed) {
                findings.insert(format!(
                    "{deployed} is deployed in {name} and routed by nothing"
                ));
            }
        }
    }
    if deployments == 0 {
        findings.insert(
            "no Deployment was found in any manifest, so the cluster was not read".to_string(),
        );
    }
    findings.into_iter().collect()
}

/// Every probe path the cluster asks for, and whether the engine answers it.
pub fn unanswerable_probes(root: &Path) -> Vec<String> {
    let routes = gateway::engine_routes(root);
    let mut findings = BTreeSet::new();
    let mut probes = 0usize;
    for (name, text) in manifests(root) {
        for line in text.lines() {
            let trimmed = line.trim_start_matches(char::is_whitespace);
            let Some(rest) = trimmed.strip_prefix("path: ") else {
                continue;
            };
            let path = rest.trim().trim_matches('"');
            if !path.starts_with('/') {
                continue;
            }
            probes += 1;
            let normalised = path.trim_end_matches('/');
            if normalised.is_empty() || normalised == "/health" {
                continue;
            }
            if !routes.contains(normalised) {
                findings.insert(format!(
                    "{name} probes {path}, which the engine does not answer"
                ));
            }
        }
    }
    if probes == 0 {
        findings.insert("no probe path was found, so nothing was checked".to_string());
    }
    findings.into_iter().collect()
}

/// Gateway entries whose address no deployment and no engine backs.
///
/// A routed service answers at `calculation-api:8099` or in its own Deployment. An entry pointing at a host that is
/// neither is a route to nothing, which a client discovers as a 502 rather than as a 404.
pub fn entries_without_a_backend(root: &Path) -> Vec<String> {
    let engine = gateway::engine_routes(root);
    let mut deployed = BTreeSet::new();
    for (_, text) in manifests(root) {
        for line in text.lines() {
            let trimmed = line.trim_start_matches(char::is_whitespace);
            if let Some(rest) = trimmed.strip_prefix("name: ") {
                deployed.insert(rest.trim().trim_matches('"').to_string());
            }
        }
    }
    let mut findings = Vec::new();
    for entry in gateway::entries(root) {
        let host = entry
            .url
            .split("//")
            .nth(1)
            .and_then(|rest| rest.split(':').next())
            .unwrap_or("")
            .to_string();
        if host.is_empty() || deployed.contains(&host) || engine_answers(&engine, &entry) {
            continue;
        }
        findings.push(format!(
            "{} forwards to {host}, which no manifest deploys",
            entry.name
        ));
    }
    findings
}

fn engine_answers(engine: &BTreeSet<String>, entry: &gateway::Entry) -> bool {
    let prefix = entry.path.trim_end_matches('/');
    let prefixed = format!("{prefix}/");
    engine.contains(prefix) || engine.iter().any(|route| route.starts_with(&prefixed))
}

/// Every address the gateway publishes, answered by the engine, is the gateway gate's job; this one is only about the
/// cluster, and the split is stated so a reader does not think one of the two covers the other.
pub fn manifest_problems(root: &Path) -> Vec<String> {
    let mut problems = deployed_but_unrouted(root);
    problems.extend(unanswerable_probes(root));
    problems.extend(entries_without_a_backend(root));
    problems
}
