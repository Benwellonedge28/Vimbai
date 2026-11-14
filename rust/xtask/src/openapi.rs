#![forbid(unsafe_code)]
//! Contracts for every published address, generated from the routes that answer them.
//!
//! Six contracts were written by hand, one per service, and a gate kept each of them honest. That was worth having and it
//! did not scale: 137 services answered 1529 addresses and six of them were described. A contract nobody serves and a
//! route nobody documented are the same defect, so the gate now demands both directions over everything.
//!
//! The arrangement keeps the detail the hand-written files carry and adds coverage for the rest:
//!
//! * `contracts/openapi/v1/services/` - the six hand-written contracts, unchanged. Their prose and schemas are the reason
//!   they are worth keeping; nothing here regenerates them.
//! * `contracts/openapi/v1/domains/` - one generated specification per domain, listing every published address of that
//!   domain that no service contract already documents, with its methods.
//!
//! A generated specification states exactly what it knows and nothing more: the path, the methods answered there, which
//! prefix group it belongs to, and the shared components - the bearer scheme, the `X-Book-Id` scope header and the one
//! error envelope. It deliberately does not invent response schemas. A schema nobody checked is a promise nobody keeps,
//! and a client reading one would be worse off than a client reading none.
//!
//! The gate, in both directions and over every address:
//!
//! * every published address appears in exactly one contract;
//! * every path a contract names is answered;
//! * every domain has a specification;
//! * the generated files are current - regenerating changes nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::catalog::{repo_root, rows};
use crate::gateway::engine_method_routes;

/// Where the hand-written per-service contracts live now.
pub const SERVICE_DIR: &str = "contracts/openapi/v1/services";
/// Where the generated per-domain specifications live.
pub const DOMAIN_DIR: &str = "contracts/openapi/v1/domains";

/// A route as the scanner reports it: `(method, path)`.
///
/// The order is the scanner's, not the reader's. Getting it the wrong way round produces five contract files named after
/// HTTP methods, which is exactly what happened the first time.
pub type Route = (String, String);

fn path_of(route: &Route) -> &str {
    &route.1
}

fn method_of(route: &Route) -> &str {
    &route.0
}

/// The file for addresses nothing can attribute.
///
/// It exists so an unattributable address is *reported* rather than dropped, and it is expected to hold nothing. Retired
/// services' prefixes used to land here by the hundred, until attribution was read from the handlers that answer them
/// (`crate::owners`): a route names its handler, the handler names the crate, the crate is documented by a domain page.
pub const UNATTRIBUTED: &str = "unattributed";

/// The prefix group an address belongs to: everything before its first segment's end.
///
/// `/accounting/accounts/{number}` belongs to `/accounting`, and `/accounting` belongs to itself. This is what the
/// gateway routes on, so it is what a contract is grouped by.
pub fn prefix_of(shape: &str) -> String {
    let mut segments = shape.trim_start_matches('/').split('/');
    match segments.next() {
        Some(first) if !first.is_empty() => format!("/{first}"),
        _ => "/".to_string(),
    }
}

/// Which domain a prefix belongs to, so a generated file is named after the domain rather than the prefix.
///
/// The catalogue answers for what the gateway routes. A retired service's prefix is not routed any more, so the catalogue
/// has nothing to say about it - but the engine still answers it, and the handler says which crate does the work. That is
/// where these come from: see [`crate::owners`]. Attribution is therefore read out of the code rather than kept in a list
/// that would go stale the moment an address moved.
fn domains_by_prefix(root: &Path) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for row in rows(root) {
        if row.gateway_path.is_empty() {
            continue;
        }
        map.entry(row.gateway_path).or_insert(row.domain);
    }
    for (prefix, owner) in crate::owners::attributed(root) {
        map.entry(prefix).or_insert(owner.domain);
    }
    map
}

/// The methods, in the order a specification should list them.
fn method_order(method: &str) -> usize {
    match method {
        "get" => 0,
        "post" => 1,
        "put" => 2,
        "patch" => 3,
        "delete" => 4,
        _ => 5,
    }
}

/// The paths a hand-written contract documents, read from its `paths:` map.
pub fn documented_paths(text: &str) -> BTreeSet<String> {
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
        if !line.starts_with(' ') {
            if !line.trim().is_empty() {
                inside = false;
            }
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with('/') {
            paths.insert(crate::gateway::shape(trimmed.trim_end_matches(':').trim()));
        }
    }
    paths
}

fn files_in(directory: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
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

/// Every path documented by a hand-written service contract.
fn service_paths(root: &Path) -> BTreeSet<String> {
    let directory = root.join(SERVICE_DIR);
    let mut documented = BTreeSet::new();
    for name in files_in(&directory) {
        let text = std::fs::read_to_string(directory.join(&name))
            .unwrap_or_else(|problem| panic!("{name} is unreadable: {problem}"));
        documented.extend(documented_paths(&text));
    }
    documented
}

/// An operation id a client generator can use: the method and the path, camel cased.
fn operation_id(method: &str, path: &str) -> String {
    let mut name = method.to_lowercase();
    for part in path.split(|character: char| !character.is_alphanumeric()) {
        let mut characters = part.chars();
        if let Some(first) = characters.next() {
            name.extend(first.to_uppercase());
            name.push_str(characters.as_str());
        }
    }
    name
}

/// The generated text of one domain's specification.
fn specification(domain: &str, routes: &BTreeSet<Route>) -> String {
    let mut prefixes: BTreeSet<String> = BTreeSet::new();
    for route in routes {
        prefixes.insert(prefix_of(path_of(route)));
    }
    let mut grouped: BTreeMap<String, Vec<&Route>> = BTreeMap::new();
    for route in routes {
        grouped
            .entry(path_of(route).to_string())
            .or_default()
            .push(route);
    }

    let mut out = String::new();
    out.push_str("openapi: 3.1.0\n");
    out.push_str("info:\n");
    out.push_str(&format!("  title: Vimbai {domain}\n"));
    out.push_str("  version: \"1.0.0\"\n");
    out.push_str(&format!(
        "  summary: The addresses the {domain} domain answers, generated from the engine's route table.\n"
    ));
    out.push_str("  description: |\n");
    out.push_str(
        "    Generated by `cargo run -p xtask -- openapi` from the routes the engine answers. Do not edit by hand:\n    regenerating is how it changes.\n\n",
    );
    out.push_str(
        "    What is stated here is what the generator can know: the path, the methods answered on it, and the\n    components every Vimbai service shares. Response schemas are not generated, because a schema nobody\n    checked is a promise nobody keeps - the wire shapes are pinned by the per-domain HTTP tests instead.\n\n",
    );
    if domain == UNATTRIBUTED {
        out.push_str(
            "    These addresses answer, and a client may call them, but no service in `catalog/services` claims their\n    prefix: they are the retired services' own addresses, still answered from the same handlers so that a client\n    which was never repointed keeps working. They are collected here rather than given a file each because a file\n    named after a prefix nobody maintains documents nothing. Attaching them to the catalogue is an open task.\n\n",
        );
    }
    out.push_str("    The addresses this file covers, by prefix group:\n");
    for prefix in &prefixes {
        out.push_str(&format!("    * `{prefix}`\n"));
    }
    out.push_str("servers:\n  - url: https://api.vimbai.example\n");
    out.push_str("security:\n  - bearerAuth: []\n");
    // The shared parts are stated once. A generated operation that repeated five response bodies inline would be a
    // document nobody reads and a diff nobody can review: one address added would move a hundred lines.
    out.push_str("components:\n");
    out.push_str("  securitySchemes:\n");
    out.push_str(
        "    bearerAuth:\n      type: http\n      scheme: bearer\n      bearerFormat: JWT\n",
    );
    out.push_str("  parameters:\n");
    out.push_str("    BookId:\n      name: X-Book-Id\n      in: header\n      required: true\n");
    out.push_str("      description: The Book the caller is asking about. A token for one Book cannot read another.\n");
    out.push_str("      schema:\n        type: string\n");
    out.push_str("  schemas:\n");
    out.push_str("    Error:\n      type: object\n      required: [error]\n");
    out.push_str("      description: The one error shape every Vimbai address answers with.\n");
    out.push_str("      properties:\n        error:\n          type: string\n");
    out.push_str("  responses:\n");
    out.push_str(
        "    Answered:\n      description: The answer the handler gives.\n      content:\n        application/json:\n          schema: {}\n",
    );
    out.push_str(
        "    Unauthorized:\n      description: No usable token.\n      content:\n        application/json:\n          schema:\n            $ref: \"#/components/schemas/Error\"\n",
    );
    out.push_str(
        "    Forbidden:\n      description: Not this caller's Book.\n      content:\n        application/json:\n          schema:\n            $ref: \"#/components/schemas/Error\"\n",
    );
    out.push_str(
        "    NotFound:\n      description: No such record.\n      content:\n        application/json:\n          schema:\n            $ref: \"#/components/schemas/Error\"\n",
    );
    out.push_str(
        "    Refused:\n      description: The request was understood and refused.\n      content:\n        application/json:\n          schema:\n            $ref: \"#/components/schemas/Error\"\n",
    );
    out.push_str("paths:\n");
    for (path, methods) in grouped {
        out.push_str(&format!("  {path}:\n"));
        out.push_str("    parameters:\n      - $ref: \"#/components/parameters/BookId\"\n");
        let mut ordered: Vec<&&Route> = methods.iter().collect();
        ordered.sort_by_key(|route| method_order(method_of(route)));
        for route in ordered {
            let method = method_of(route).to_lowercase();
            out.push_str(&format!("    {method}:\n"));
            out.push_str(&format!(
                "      operationId: {}\n",
                operation_id(method_of(route), &path)
            ));
            out.push_str(
                "      responses: { \"200\": { $ref: \"#/components/responses/Answered\" }, \"401\": { $ref: \"#/components/responses/Unauthorized\" }, \"403\": { $ref: \"#/components/responses/Forbidden\" }, \"404\": { $ref: \"#/components/responses/NotFound\" }, \"422\": { $ref: \"#/components/responses/Refused\" } }\n",
            );
        }
    }
    out
}

/// The generated files the repository should hold, domain by domain.
///
/// A domain with nothing left to document has no file: an empty specification would satisfy a coverage check while
/// telling a reader nothing, which is the failure mode this exists to prevent.
pub fn expected(root: &Path) -> BTreeMap<String, String> {
    let covered = service_paths(root);
    let by_prefix = domains_by_prefix(root);
    let mut per_domain: BTreeMap<String, BTreeSet<Route>> = BTreeMap::new();
    for route in engine_method_routes(root) {
        if covered.contains(&route.1) {
            continue;
        }
        let domain = by_prefix
            .get(&prefix_of(&route.1))
            .cloned()
            // An address whose prefix no catalogue row claims is still an address. It is filed together, under a name
            // that says what it is, rather than dropped or given a file of its own: see `UNATTRIBUTED`.
            .unwrap_or_else(|| UNATTRIBUTED.to_string());
        per_domain.entry(domain).or_default().insert(route);
    }
    per_domain
        .into_iter()
        .map(|(domain, routes)| (format!("{domain}.yaml"), specification(&domain, &routes)))
        .collect()
}

/// Write the generated specifications, and remove the ones for domains that no longer exist.
pub fn write(root: &Path) -> Result<usize, String> {
    let directory = root.join(DOMAIN_DIR);
    std::fs::create_dir_all(&directory).map_err(|problem| problem.to_string())?;
    let expected = expected(root);
    for (name, text) in &expected {
        std::fs::write(directory.join(name), text).map_err(|problem| problem.to_string())?;
    }
    for name in files_in(&directory) {
        if !expected.contains_key(&name) {
            std::fs::remove_file(directory.join(&name)).map_err(|problem| problem.to_string())?;
        }
    }
    Ok(expected.len())
}

/// A generated file that is not what the generator would write now.
pub fn stale(root: &Path) -> Vec<String> {
    let directory = root.join(DOMAIN_DIR);
    let expected = expected(root);
    let mut found = Vec::new();
    for (name, text) in &expected {
        match std::fs::read_to_string(directory.join(name)) {
            Ok(held) if &held == text => {}
            Ok(_) => found.push(format!(
                "{DOMAIN_DIR}/{name} is out of date: run `cargo run -p xtask -- openapi`"
            )),
            Err(_) => found.push(format!(
                "{DOMAIN_DIR}/{name} is missing: run `cargo run -p xtask -- openapi`"
            )),
        }
    }
    for name in files_in(&directory) {
        if !expected.contains_key(&name) {
            found.push(format!(
                "{DOMAIN_DIR}/{name} documents no address the engine answers"
            ));
        }
    }
    found
}

/// Both directions, over every published address.
pub fn problems(root: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let engine = engine_method_routes(root);
    if engine.is_empty() {
        problems.push("the engine answers nothing, so no contract could be checked".to_string());
        return problems;
    }
    let answered: BTreeSet<String> = engine.iter().map(|(_, path)| path.clone()).collect();

    // every path a contract names must be answered, and no path may be claimed twice
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for directory in [SERVICE_DIR, DOMAIN_DIR] {
        for name in files_in(&root.join(directory)) {
            let text = std::fs::read_to_string(root.join(directory).join(&name))
                .unwrap_or_else(|problem| panic!("{name} is unreadable: {problem}"));
            let paths = documented_paths(&text);
            if paths.is_empty() {
                problems.push(format!("{directory}/{name} documents no paths"));
                continue;
            }
            for path in paths {
                if !answered.contains(&path) {
                    problems.push(format!(
                        "{directory}/{name} documents {path}, which the engine does not answer"
                    ));
                }
                if let Some(first) = seen.get(&path) {
                    problems.push(format!(
                        "{path} is documented in both {first} and {directory}/{name}"
                    ));
                } else {
                    seen.insert(path, format!("{directory}/{name}"));
                }
            }
        }
    }

    // and every published address must be in one
    for path in &answered {
        if !seen.contains_key(path) {
            problems.push(format!(
                "{path} is served but no contract documents it: run `cargo run -p xtask -- openapi` if it belongs to a generated domain"
            ));
        }
    }

    // the generated files must be current, or the coverage above is describing files nobody regenerates
    problems.extend(stale(root));
    problems.sort();
    problems.dedup();
    problems
}

/// The repository root, for the CLI.
pub fn root() -> std::path::PathBuf {
    repo_root()
}
