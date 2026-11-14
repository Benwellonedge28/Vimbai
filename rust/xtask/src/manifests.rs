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

/// Every manifest in the cluster tree, named by its path inside it.
///
/// The tree is `base/` plus `overlays/`, so this reads it whole rather than one directory deep: a per-service Deployment
/// hiding in an overlay is exactly the drift the gate exists to catch, and a scan that only looked at the top level would
/// have found nothing there and called it clean.
pub fn all(root: &Path) -> Vec<(String, String)> {
    manifests(root)
}

fn manifests(root: &Path) -> Vec<(String, String)> {
    let directory = root.join("infrastructure/kubernetes");
    let mut found = Vec::new();
    read_manifests(&directory, &directory, &mut found);
    found.sort();
    found
}

fn read_manifests(base: &Path, directory: &Path, found: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            read_manifests(base, &path, found);
            continue;
        }
        if !path
            .extension()
            .is_some_and(|extension| extension == "yaml" || extension == "yml")
        {
            continue;
        }
        let Some(name) = path
            .strip_prefix(base)
            .ok()
            .map(|rest| rest.to_string_lossy().replace('\\', "/"))
        else {
            continue;
        };
        if let Ok(text) = std::fs::read_to_string(&path) {
            found.push((name, text));
        }
    }
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
        let mut previous = String::new();
        for line in text.lines() {
            let trimmed = line.trim_start_matches(char::is_whitespace);
            // Only a path a probe asks for counts. A kustomize patch is written as a JSON pointer under `patch: |-` and
            // its lines also read `path: /spec/replicas`, which an earlier version of this scan took for an address and
            // reported as a probe the engine does not answer.
            let asked_by_a_probe = previous == "httpGet:" || previous.starts_with("httpGet:");
            previous = trimmed.trim_end().to_string();
            if !asked_by_a_probe {
                continue;
            }
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

/// The cluster is `base/` plus `overlays/`, and a file at the top level is a manifest nothing applies.
///
/// The tree used to be flat, so "where does this go" had no answer and everything went to the top. The split is worth
/// holding because the difference between a base manifest and an overlay is the difference between something every
/// environment gets and something one environment gets, and a file in the wrong place is silently the wrong one of those.
pub fn split_problems(root: &Path) -> Vec<String> {
    let directory = root.join("infrastructure/kubernetes");
    let mut problems = Vec::new();
    if !directory.join("base/kustomization.yaml").is_file() {
        problems.push("infrastructure/kubernetes/base/kustomization.yaml is missing".to_string());
    }
    let overlays = directory.join("overlays");
    let mut overlay_count = 0usize;
    if let Ok(entries) = std::fs::read_dir(&overlays) {
        for entry in entries.filter_map(Result::ok) {
            if !entry.path().is_dir() {
                continue;
            }
            overlay_count += 1;
            if !entry.path().join("kustomization.yaml").is_file() {
                problems.push(format!(
                    "overlays/{} has no kustomization.yaml, so nothing renders it",
                    entry.file_name().to_string_lossy()
                ));
            }
        }
    }
    if overlay_count == 0 {
        problems.push("no overlay directory exists, so nothing was checked".to_string());
    }
    let mut top_level = 0usize;
    if let Ok(entries) = std::fs::read_dir(&directory) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }
            if path
                .extension()
                .is_some_and(|extension| extension == "yaml" || extension == "yml")
            {
                top_level += 1;
                problems.push(format!(
                    "{} sits at the top of the cluster tree instead of in base/ or an overlay",
                    entry.file_name().to_string_lossy()
                ));
            }
        }
    }
    if top_level == 0 && problems.is_empty() {
        // nothing to say: base exists, every overlay renders, and nothing is loose at the top
    }
    problems
}

/// Every file a kustomization names has to exist, or `kubectl apply -k` fails on a machine that is not this one.
pub fn kustomization_problems(root: &Path) -> Vec<String> {
    let directory = root.join("infrastructure/kubernetes");
    let mut problems = Vec::new();
    let mut checked = 0usize;
    let mut found = Vec::new();
    read_manifests(&directory, &directory, &mut found);
    for (name, text) in found {
        if !name.ends_with("kustomization.yaml") {
            continue;
        }
        let base = directory.join(Path::new(&name).parent().unwrap_or_else(|| Path::new("")));
        let mut in_resources = false;
        let mut resources_indent = 0usize;
        for line in text.lines() {
            let trimmed = line.trim();
            let indent = line.len() - line.trim_start().len();
            if trimmed == "resources:" {
                in_resources = true;
                resources_indent = indent;
                continue;
            }
            if !in_resources {
                continue;
            }
            // the list ends where the next key starts: a `patches:`, an `images:`, a literal somewhere else
            if !trimmed.is_empty() && !trimmed.starts_with('-') && indent <= resources_indent {
                in_resources = false;
                continue;
            }
            if !trimmed.starts_with("- ") {
                continue;
            }
            let entry = trimmed.trim_start_matches("- ").trim();
            if entry.is_empty() || entry.starts_with('#') {
                continue;
            }
            let target = base.join(entry);
            if target.exists() {
                checked += 1;
            } else {
                problems.push(format!("{name} names {entry}, which does not exist"));
            }
        }
    }
    if checked == 0 {
        problems.push("no kustomization resource was found, so nothing was checked".to_string());
    }
    problems
}

/// The packages and binaries the workspace actually declares, so a compose file cannot name one that is not there.
fn declared_bins(root: &Path) -> Vec<(String, Vec<String>)> {
    let mut packages = Vec::new();
    let mut stack = vec![root.join("rust")];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "target") {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if path.file_name().is_some_and(|name| name == "Cargo.toml") {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if let Some(package) = package_bins(&text) {
                    packages.push(package);
                }
            }
        }
    }
    packages
}

/// A manifest's package name and its binaries, read by line shape: no YAML or TOML parser lives in this tool.
fn package_bins(text: &str) -> Option<(String, Vec<String>)> {
    let mut name: Option<String> = None;
    let mut bins = Vec::new();
    let mut in_bin = false;
    let mut in_package = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[[bin]]") {
            in_bin = true;
            in_package = false;
            continue;
        }
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            if !trimmed.starts_with("[[bin]]") {
                in_bin = false;
            }
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("name") else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_string();
        if in_package && name.is_none() {
            name = Some(value);
        } else if in_bin {
            bins.push(value);
        }
    }
    let name = name?;
    // a package with no [[bin]] builds one binary named after itself
    if bins.is_empty() {
        bins.push(name.replace('-', "_"));
    }
    Some((name, bins))
}

/// The compose estate, and the images it builds.
///
/// Two things went wrong in the file this replaced: it described services that no longer existed, and it pointed at
/// Dockerfiles that were somewhere else. Neither is visible without running docker, so both are read here by line shape -
/// a build context that exists, a Dockerfile that exists, and a package and binary the workspace declares.
pub fn compose_problems(root: &Path) -> Vec<String> {
    let directory = root.join("infrastructure/compose");
    let packages = declared_bins(root);
    let mut problems = Vec::new();
    let mut services = 0usize;
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return vec!["infrastructure/compose does not exist".to_string()];
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml" || extension == "yml")
        })
        .collect();
    files.sort();
    for file in &files {
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        let lines: Vec<&str> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            if line.trim() != "build:" {
                continue;
            }
            services += 1;
            let indent = line.len() - line.trim_start().len();
            let mut context: Option<String> = None;
            let mut dockerfile: Option<String> = None;
            let mut package: Option<String> = None;
            let mut binary: Option<String> = None;
            for inside in lines.iter().skip(index + 1) {
                let inside_indent = inside.len() - inside.trim_start().len();
                if !inside.trim().is_empty() && inside_indent <= indent {
                    break;
                }
                let trimmed = inside.trim();
                for (key, slot) in [
                    ("context:", &mut context),
                    ("dockerfile:", &mut dockerfile),
                    ("PACKAGE:", &mut package),
                    ("BIN:", &mut binary),
                ] {
                    if let Some(value) = trimmed.strip_prefix(key) {
                        *slot = Some(value.trim().trim_matches('"').to_string());
                    }
                }
            }
            let Some(context) = context else {
                problems.push(format!("{name}: a build has no context"));
                continue;
            };
            let context_path = directory.join(&context);
            if !context_path.is_dir() {
                problems.push(format!(
                    "{name}: the build context {context} does not exist"
                ));
            }
            if let Some(dockerfile) = &dockerfile {
                if !context_path.join(dockerfile).is_file() {
                    problems.push(format!(
                        "{name}: {dockerfile} is not beside the context {context}"
                    ));
                }
            }
            if let Some(package) = &package {
                let Some((_, bins)) = packages.iter().find(|(known, _)| known == package) else {
                    problems.push(format!(
                        "{name}: {package} is not a package in the workspace"
                    ));
                    continue;
                };
                if let Some(binary) = &binary {
                    if !bins.iter().any(|known| known == binary) {
                        problems.push(format!(
                            "{name}: {package} declares no binary {binary} (it has {bins:?})"
                        ));
                    }
                }
            }
        }
    }
    if services == 0 {
        problems.push("no compose service builds anything, so nothing was checked".to_string());
    }
    problems
}

/// Every Dockerfile in the image directory is used by something, and nothing builds from a Dockerfile outside it.
pub fn image_problems(root: &Path) -> Vec<String> {
    let directory = root.join("infrastructure/docker");
    let mut problems = Vec::new();
    let mut dockerfiles: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&directory) {
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("Dockerfile") {
                dockerfiles.push(name);
            }
        }
    }
    if dockerfiles.is_empty() {
        return vec!["infrastructure/docker holds no Dockerfile".to_string()];
    }
    // everything that could name one: the compose estate and the workflows
    let mut named = String::new();
    for tree in ["infrastructure/compose", ".github/workflows"] {
        let mut stack = vec![root.join(tree)];
        while let Some(directory) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(text) = std::fs::read_to_string(&path) {
                    named.push_str(&text);
                }
            }
        }
    }
    for dockerfile in dockerfiles {
        if !named.contains(&dockerfile) {
            problems.push(format!(
                "infrastructure/docker/{dockerfile} is built by nothing"
            ));
        }
    }
    // and the other way round: a workflow naming a Dockerfile that is not here
    for line in named.lines() {
        let trimmed = line.trim();
        let Some(index) = trimmed.find("infrastructure/docker/") else {
            continue;
        };
        let reference = &trimmed[index + "infrastructure/docker/".len()..];
        let file: String = reference
            .chars()
            .take_while(|character| !character.is_whitespace() && *character != '"')
            .collect();
        if file.is_empty() {
            continue;
        }
        if !directory.join(&file).is_file() {
            problems.push(format!("{file} is named as an image but does not exist"));
        }
    }
    problems.sort();
    problems.dedup();
    problems
}

/// Every address the gateway publishes, answered by the engine, is the gateway gate's job; this one is only about the
/// cluster, and the split is stated so a reader does not think one of the two covers the other.
pub fn manifest_problems(root: &Path) -> Vec<String> {
    let mut problems = deployed_but_unrouted(root);
    problems.extend(unanswerable_probes(root));
    problems.extend(entries_without_a_backend(root));
    problems.extend(split_problems(root));
    problems.extend(kustomization_problems(root));
    problems.extend(compose_problems(root));
    problems.extend(image_problems(root));
    problems
}
