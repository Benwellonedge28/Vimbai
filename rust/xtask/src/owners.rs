#![forbid(unsafe_code)]
//! Which domain answers an address the catalogue no longer has a service for.
//!
//! Retiring a service moved its addresses onto the engine without moving them in the gateway, so the client that was never
//! repointed keeps working. The catalogue describes what the *gateway* routes, and these addresses are answered but not
//! routed, so no catalogue row owns them - and the contract generator, which groups specifications by owner, had nowhere to
//! file them. That produced one honest bucket called `unattributed` holding 180 prefix groups, which documented the gap
//! without closing it.
//!
//! The ownership is not lost, it is just written in a different place: the route says which handler answers it, and the
//! handler says which crate it delegates to. That is what this module reads. Nothing here is maintained by hand - a new
//! address is attributed by being written the way every other address is written, and a handler that stops delegating stops
//! being attributed, which is the correct answer once the code it delegated to is gone.
//!
//! The rules, in order:
//!
//! * an address belongs to the crate its handler delegates to (`compliance::sox::sox_dashboard` -> `vimbai_compliance`);
//! * a prefix group belongs to the crate most of its addresses name, and a group whose addresses disagree is reported
//!   rather than silently settled;
//! * a crate belongs to the domain whose page names it, and a crate no page names is reported, because an owner nobody can
//!   read about is not much of an owner.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::catalog::{repo_root, rows};
use crate::gateway::{call_arguments, engine_sources, first_literal};
use crate::openapi::prefix_of;

/// A prefix group, the crate that answers it and the domain whose page covers that crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    pub krate: String,
    pub domain: String,
    /// How many addresses of the group the crate answers, so a group split between two crates shows its weight.
    pub addresses: usize,
}

impl Owner {
    pub fn new(krate: impl Into<String>, domain: impl Into<String>, addresses: usize) -> Self {
        Self {
            krate: krate.into(),
            domain: domain.into(),
            addresses,
        }
    }
}

/// Every prefix group the engine answers, attributed where its handlers say who answers.
///
/// The catalogue's own prefixes are not included: a routed service already has an owner and this must not second-guess it.
pub fn attributed(root: &Path) -> BTreeMap<String, Owner> {
    let routed: BTreeSet<String> = rows(root)
        .into_iter()
        .map(|row| row.gateway_path)
        .filter(|path| !path.is_empty())
        .collect();
    let sources = engine_sources(root);
    let crates = crate_paths(root);
    let aliases = aliases(&sources, &crates);
    let page_domains = domains_by_prefix_in_pages();
    let domains = domains_by_crate();
    let documented: BTreeSet<String> = crates
        .iter()
        .filter(|(_, path)| domains.contains_key(*path))
        .map(|(name, _)| name.clone())
        .collect();
    let answered = crate::gateway::engine_routes(root);

    // crate per prefix, counted across the addresses that prefix answers
    let mut counted: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for text in &sources {
        let mounts = crate::gateway::mounts_for(text);
        let mut from = 0usize;
        while let Some(offset) = text[from..].find(".route(") {
            let open_at = from + offset + ".route".len();
            from = open_at;
            let Some(arguments) = call_arguments(text, open_at) else {
                continue;
            };
            let Some(literal) = first_literal(arguments) else {
                continue;
            };
            let Some(krate) = crate_answering(&sources, &aliases, &documented, arguments) else {
                continue;
            };
            for (prefix, shape) in addresses_of(arguments, &literal, &mounts) {
                // an address the route table does not actually answer is not an address, and a prefix of "/{}" from a
                // stray literal is exactly that
                if !answered.contains(&shape) || routed.contains(&prefix) {
                    continue;
                }
                *counted
                    .entry(prefix)
                    .or_default()
                    .entry(krate.clone())
                    .or_insert(0) += 1;
            }
        }
    }

    let mut owners = BTreeMap::new();
    for (prefix, by_crate) in counted {
        // the crate most of the group's addresses name; a tie keeps the first, and a split group is reported by `split`
        let Some((krate, addresses)) = by_crate
            .iter()
            .max_by(|left, right| left.1.cmp(right.1).then(right.0.cmp(left.0)))
        else {
            continue;
        };
        let path = crates.get(krate).cloned().unwrap_or_default();
        let documented = domains.get(&path).cloned();
        let domain = match documented {
            Some(domain) => domain,
            // the crate answers it and no page documents the crate: the page that lists the prefix is the owner of record
            // The crate answers it and no page documents the crate: the prefix names the file, which is the one thing about
            // the address that is not a guess. Filing it under a passing mention of the prefix in another page's prose is
            // how `/identity` was once attributed to the organisation pages.
            None => page_domains
                .get(&prefix)
                .cloned()
                .unwrap_or_else(|| prefix.trim_start_matches('/').replace('/', "_")),
        };
        owners.insert(prefix, Owner::new(krate.clone(), domain, *addresses));
    }
    owners
}

/// Prefix groups whose addresses do not agree on which crate answers them, with the crate each one names.
pub fn split(root: &Path) -> BTreeMap<String, BTreeSet<String>> {
    let sources = engine_sources(root);
    let aliases = aliases(&sources, &crate_paths(root));
    let answered = crate::gateway::engine_routes(root);
    let domains = domains_by_crate();
    let documented: BTreeSet<String> = crate_paths(root)
        .into_iter()
        .filter(|(_, path)| domains.contains_key(path))
        .map(|(name, _)| name)
        .collect();
    let mut seen: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for text in &sources {
        let mut from = 0usize;
        while let Some(offset) = text[from..].find(".route(") {
            let open_at = from + offset + ".route".len();
            from = open_at;
            let Some(arguments) = call_arguments(text, open_at) else {
                continue;
            };
            let Some(literal) = first_literal(arguments) else {
                continue;
            };
            let shape = crate::gateway::shape(&literal);
            if !answered.contains(&shape) {
                continue;
            }
            let Some(krate) = crate_answering(&sources, &aliases, &documented, arguments) else {
                continue;
            };
            seen.entry(prefix_of(&shape)).or_default().insert(krate);
        }
    }
    seen.into_iter()
        .filter(|(_, crates)| crates.len() > 1)
        .collect()
}

/// Addresses the engine answers that neither the catalogue nor a handler's own delegation can attribute.
pub fn unattributed(root: &Path, addresses: &BTreeSet<String>) -> BTreeSet<String> {
    let owned = attributed(root);
    addresses
        .iter()
        .filter(|path| {
            let prefix = prefix_of(path);
            !owned.contains_key(&prefix)
                && !rows(root)
                    .iter()
                    .any(|row| row.gateway_path == prefix && !row.gateway_path.is_empty())
        })
        .cloned()
        .collect()
}

/// Every name the route table can call a crate by, mapped to the crate's package name.
///
/// The engine renames its dependencies so the code reads as the domain rather than as a package (`accounting` for
/// `vimbai_accounting`), so the manifest is where the two are tied together. Reading it means a crate that is renamed again
/// is still attributed, instead of the scanner holding a list of names that were true once.
fn aliases(sources: &[String], crates: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    // only a crate in this workspace can answer an address: `axum` and `serde_json` are called by handlers and own nothing
    let mut found: BTreeMap<String, String> = dependencies()
        .into_iter()
        .filter(|(_, package)| crates.contains_key(package))
        .collect();
    for text in sources {
        for line in text.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("use ") else {
                continue;
            };
            let name = rest
                .split(|character: char| {
                    character == ':' || character == '{' || character == ' ' || character == ';'
                })
                .next()
                .unwrap_or_default();
            if !name.starts_with("vimbai_") {
                continue;
            }
            let alias = match rest.find(" as ") {
                Some(at) => rest[at + 4..]
                    .split([';', ' '])
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                None => name.trim_start_matches("vimbai_").to_string(),
            };
            if !alias.is_empty() {
                found.insert(alias, name.to_string());
            }
        }
    }
    found
}

/// The address a route answers, as `(prefix, shape)` pairs.
///
/// A `format!` route is spliced once per mount it is declared under - `/identity` and `/records/identity` are the same
/// handler published twice - so one route can be several addresses and each is attributed separately.
fn addresses_of(arguments: &str, literal: &str, mounts: &[String]) -> Vec<(String, String)> {
    if !arguments[..arguments.find('"').unwrap_or(0)].contains("format!") {
        let shape = crate::gateway::shape(literal);
        return vec![(prefix_of(&shape), shape)];
    }
    let Some(suffix) = literal.strip_prefix("{prefix}") else {
        return Vec::new();
    };
    mounts
        .iter()
        .map(|mount| {
            let shape = crate::gateway::shape(&format!("{mount}{suffix}"));
            (prefix_of(mount), shape)
        })
        .collect()
}

/// The crate answering a route, whether it names a handler or is the handler.
fn crate_answering(
    sources: &[String],
    aliases: &BTreeMap<String, String>,
    documented: &BTreeSet<String>,
    arguments: &str,
) -> Option<String> {
    let named = first_handler(arguments)
        .and_then(|handler| crate_of(sources, aliases, documented, &handler));
    named
        // a route whose handler is written inline - a closure over one call - says which crate it delegates to in its own
        // arguments, or in the function those arguments call
        .or_else(|| crate_named_in(arguments, aliases, documented))
        .or_else(|| {
            called_by(arguments)
                .into_iter()
                .find_map(|call| crate_of(sources, aliases, documented, &call))
        })
}

/// The functions a piece of source calls, in the order they appear.
fn called_by(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find('(') {
        let before = &rest[..at];
        let name: String = before
            .chars()
            .rev()
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect();
        if !name.is_empty() && !["get", "post", "put", "delete", "patch"].contains(&name.as_str()) {
            found.push(name);
        }
        rest = &rest[at + 1..];
    }
    found
}

/// The first `<alias>::` in a piece of source, as the crate that alias names.
fn crate_named_in(
    text: &str,
    aliases: &BTreeMap<String, String>,
    documented: &BTreeSet<String>,
) -> Option<String> {
    // A handler reads the store before it does anything: `store::Records` comes first, and it owns no domain. So the
    // earliest crate *that a domain page documents* wins, and only when nothing documented is named does the earliest
    // crate at all answer - which is the honest answer for a door nothing has written a page about yet.
    let earliest = |only_documented: bool| -> Option<(usize, String)> {
        let mut best: Option<(usize, String)> = None;
        for (alias, krate) in aliases {
            if only_documented && !documented.contains(krate) {
                continue;
            }
            let call = format!("{alias}::");
            if let Some(position) = text.find(&call) {
                if best
                    .as_ref()
                    .map(|(seen, _)| position < *seen)
                    .unwrap_or(true)
                {
                    best = Some((position, krate.clone()));
                }
            }
        }
        best
    };
    earliest(true)
        .or_else(|| earliest(false))
        .map(|(_, krate)| krate)
}

/// `alias = { package = "vimbai_x" }` from the route table's own manifest, alias -> package.
fn dependencies() -> BTreeMap<String, String> {
    let manifest = repo_root().join("rust/apps/api-server/Cargo.toml");
    let mut found = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(&manifest) else {
        return found;
    };
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            inside = trimmed == "[dependencies]";
            continue;
        }
        if !inside || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, rest)) = trimmed.split_once('=') else {
            continue;
        };
        let alias = key.trim().to_string();
        if alias.is_empty() {
            continue;
        }
        let package = match rest.find("package = ") {
            Some(at) => rest[at + "package = ".len()..]
                .trim_start()
                .trim_start_matches('"')
                .split('"')
                .next()
                .unwrap_or_default()
                .to_string(),
            None => alias.clone(),
        };
        if package.starts_with("vimbai_") || package == alias {
            found.insert(alias, package);
        }
    }
    found
}

/// The handler an expression binds first: the name inside the first `get(`, `post(` and so on.
fn first_handler(arguments: &str) -> Option<String> {
    let mut earliest: Option<(usize, String)> = None;
    for method in ["get", "post", "put", "delete", "patch"] {
        let needle = format!("{method}(");
        if let Some(at) = arguments.find(&needle) {
            let rest = &arguments[at + needle.len()..];
            let name: String = rest
                .chars()
                .take_while(|character| {
                    character.is_alphanumeric() || *character == '_' || *character == ':'
                })
                .collect();
            let name = name.trim_end_matches(':').to_string();
            if name.is_empty() {
                continue;
            }
            if earliest
                .as_ref()
                .map(|(seen, _)| at < *seen)
                .unwrap_or(true)
            {
                earliest = Some((at, name));
            }
        }
    }
    earliest.map(|(_, name)| name)
}

/// The crate a handler delegates to: the first `alias::` its body names.
fn crate_of(
    sources: &[String],
    aliases: &BTreeMap<String, String>,
    documented: &BTreeSet<String>,
    handler: &str,
) -> Option<String> {
    let needle = format!("fn {handler}(");
    for text in sources {
        let Some(at) = text.find(&needle) else {
            continue;
        };
        // a handler is a wrapper: its body names the crate that does the work before it names anything else
        if let Some(krate) = crate_named_in(&text[at..], aliases, documented) {
            return Some(krate);
        }
    }
    None
}

/// Every crate in the workspace by package name, so a name in a `use` resolves to the path a page would name.
fn crate_paths(root: &Path) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for base in ["rust/crates", "rust/apps", "rust/tests"] {
        walk(&root.join(base), root, &mut found, 0);
    }
    found
}

fn walk(directory: &Path, root: &Path, found: &mut BTreeMap<String, String>, depth: usize) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, root, found, depth + 1);
            continue;
        }
        if path.file_name().and_then(|name| name.to_str()) != Some("Cargo.toml") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(name) = text.lines().find_map(|line| {
            line.trim()
                .strip_prefix("name = ")
                .map(|value| value.trim().trim_matches('"').to_string())
        }) else {
            continue;
        };
        let Some(parent) = path.parent() else {
            continue;
        };
        let Ok(relative) = parent.strip_prefix(root) else {
            continue;
        };
        found.insert(name, relative.to_string_lossy().into_owned());
    }
}

/// The domain each prefix belongs to, read from the page that lists it.
///
/// A domain page names the addresses its domain answers, so a prefix a page lists has an owner even when the handler behind
/// it names nothing a page documents - which is what happens where a door reads the store and then does its work inline.
fn domains_by_prefix_in_pages() -> BTreeMap<String, String> {
    let directory = repo_root().join("docs/domains");
    let mut found = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return found;
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".md"))
        .collect();
    names.sort();
    for name in names {
        let Ok(text) = std::fs::read_to_string(directory.join(&name)) else {
            continue;
        };
        let domain = name.trim_end_matches(".md").to_string();
        // A page owns a prefix when the prefix is the *subject* of a line - a headline, a bullet or a table's first column -
        // and not merely when the page mentions it. `/identity` appears in the organisation pages' prose and belongs to
        // neither of them.
        for line in text.lines() {
            let trimmed = line
                .trim_start()
                .trim_start_matches(['|', '-', '*', '#', ' '])
                .trim_start();
            let Some(rest) = trimmed.strip_prefix('`') else {
                continue;
            };
            let token = rest.split('`').next().unwrap_or_default().trim();
            if !token.starts_with('/') || token.contains(' ') {
                continue;
            }
            let first = token.trim_end_matches('/');
            found.entry(first.to_string()).or_insert(domain.clone());
        }
    }
    found
}

/// The domain each crate belongs to, read from the page that documents it.
fn domains_by_crate() -> BTreeMap<String, String> {
    let root = repo_root();
    let directory = root.join("docs/domains");
    let mut found = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return found;
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".md"))
        .collect();
    names.sort();
    for name in names {
        let Ok(text) = std::fs::read_to_string(directory.join(&name)) else {
            continue;
        };
        let domain = name.trim_end_matches(".md").to_string();
        // the page states its crates on one line: `Crate: `rust/crates/domains/compliance`, ...`
        for line in text.lines() {
            if !line.starts_with("Crate:") {
                continue;
            }
            for piece in line.split('`') {
                let piece = piece.trim_end_matches(',').trim();
                if piece.starts_with("rust/crates/") || piece.starts_with("rust/apps/") {
                    found.entry(piece.to_string()).or_insert(domain.clone());
                }
            }
        }
    }
    found
}
