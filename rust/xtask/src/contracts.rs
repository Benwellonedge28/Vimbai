//! A contract is only worth having if it is checked against the code it describes.
//!
//! `contracts/*.yaml` is written by hand from the handlers. Hand-written files drift, so this compares each one, in
//! both directions, to the Rust that answers its routes: every path the contract publishes is a route the engine
//! answers, and every route of that service appears in its contract.
//!
//! What this does not do, stated rather than implied: the Python version also parsed the schemas, checking that money
//! fields are decimal strings and that a documented response names the keys the handler writes. That needs a YAML
//! parser, and the workspace carries none on purpose - the tooling builds on a machine with no network. The schemas are
//! unchanged and the wire shapes are pinned by the per-domain HTTP tests; what is lost here is the automatic tie between
//! a documented response key and the key the handler writes.
#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::path::Path;

use crate::gateway::{engine_routes, shape};

/// One hand-written contract, and the service surface it describes.
struct Contract {
    file: &'static str,
    prefix: &'static str,
}

const CONTRACTS: [Contract; 6] = [
    Contract {
        file: "accounting.yaml",
        prefix: "/cash-flow-statement",
    },
    Contract {
        file: "assets.yaml",
        prefix: "/fixed-assets-register",
    },
    Contract {
        file: "costing.yaml",
        prefix: "/overhead-absorption-rate",
    },
    Contract {
        file: "equity.yaml",
        prefix: "/equity-changes",
    },
    Contract {
        file: "job-costing.yaml",
        prefix: "/job-costing",
    },
    Contract {
        file: "treasury.yaml",
        prefix: "/cash-optimization",
    },
];

/// The paths a contract publishes, read from its `paths:` map.
fn contract_paths(text: &str) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with("paths:") {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if !line.starts_with(' ') || line.trim().is_empty() {
            if !line.trim().is_empty() {
                inside = false;
            }
            continue;
        }
        let trimmed = line.trim_start();
        if !trimmed.starts_with('/') {
            continue;
        }
        let path = trimmed.trim_end_matches(':').trim();
        paths.insert(shape(path));
    }
    paths
}

fn contract_files(root: &Path) -> Vec<String> {
    let directory = root.join("contracts");
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".yaml"))
        .collect();
    names.sort();
    names
}

/// Contracts that describe an address the engine does not answer, or forget one it does.
pub fn problems(root: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let engine = engine_routes(root);
    let files = contract_files(root);
    if files.is_empty() {
        problems.push("no contract files found, so nothing was checked".to_string());
        return problems;
    }
    for contract in &CONTRACTS {
        if !files.iter().any(|name| name == contract.file) {
            problems.push(format!(
                "{} is listed here and missing from contracts/",
                contract.file
            ));
            continue;
        }
        let path = root.join("contracts").join(contract.file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|problem| panic!("{} is unreadable: {problem}", path.display()));
        let documented = contract_paths(&text);
        if documented.is_empty() {
            problems.push(format!(
                "{} publishes no paths, so it describes nothing",
                contract.file
            ));
            continue;
        }
        for path in &documented {
            if !engine.contains(path) {
                problems.push(format!(
                    "{} documents {path}, which the engine does not answer",
                    contract.file
                ));
            }
        }
        // The other direction: a route of this service the contract forgot. The /records mirror is the same handler
        // under the record prefix, so it is not a separate promise.
        for route in &engine {
            if route.starts_with("/records/") || !route.starts_with(contract.prefix) {
                continue;
            }
            if !documented.contains(route) {
                problems.push(format!(
                    "{} is served but not documented in {}",
                    route, contract.file
                ));
            }
        }
    }
    problems
}
