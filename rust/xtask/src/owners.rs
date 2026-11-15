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
        let Some((krate, addresses)) = filed(&by_crate) else {
            continue;
        };
        let path = crates.get(&krate).cloned().unwrap_or_default();
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
        owners.insert(prefix, Owner::new(krate, domain, addresses));
    }

    // Doors the engine answers itself, with no domain crate behind them - the calculator catalogue is the engine's own
    // registry, not a domain's arithmetic. Those are filed under the page whose address table claims the prefix, which is
    // an explicit claim by that page rather than a passing mention in its prose.
    for (prefix, domain) in &page_domains {
        if owners.contains_key(prefix) {
            continue;
        }
        let mut addressed = answered
            .iter()
            .filter(|shape| prefix_of(shape) == *prefix)
            .count();
        if addressed == 0 {
            continue;
        }
        if routed.contains(prefix) {
            addressed = 0;
        }
        if addressed == 0 {
            continue;
        }
        owners.insert(
            prefix.clone(),
            Owner::new("vimbai_api", domain.clone(), addressed),
        );
    }
    owners
}

/// Prefix groups whose addresses do not agree on which crate answers them, with the crate each one names.
/// Which crate a prefix group is filed under, given how many of its addresses each one answers.
///
/// The crate answering the most of the group wins, and a tie is broken by name so the answer never depends on the order the
/// scan happened to reach the crates in. A group no crate answers is filed nowhere rather than guessed at; `split` reports the
/// groups more than one crate answers, so a reader can see which prefixes overlap without the answer being decided by chance.
pub fn filed(by_crate: &BTreeMap<String, usize>) -> Option<(String, usize)> {
    by_crate
        .iter()
        .max_by(|left, right| left.1.cmp(right.1).then(right.0.cmp(left.0)))
        .map(|(krate, addresses)| (krate.clone(), *addresses))
}

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
/// Every `use` statement in a file, whitespace collapsed, however many lines it was written on.
///
/// rustfmt wraps a long import over several lines, and reading one line at a time lost the rest of it: the crate at the
/// head of `use budgeting_services::{` was seen, and the modules named below it - which is how every budgeting calculation
/// is called - were not, so those handlers looked like they delegated to nothing.
fn use_statements(text: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        match current.as_mut() {
            Some(open) => {
                open.push(' ');
                open.push_str(trimmed);
                if trimmed.ends_with(';') {
                    statements.push(open.clone());
                    current = None;
                }
            }
            None => {
                if let Some(rest) = trimmed.strip_prefix("use ") {
                    if trimmed.ends_with(';') {
                        statements.push(rest.to_string());
                    } else {
                        current = Some(rest.to_string());
                    }
                }
            }
        }
    }
    statements
}

fn aliases(sources: &[String], crates: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    // Only a crate in this workspace can answer an address: `axum` and `serde_json` are called by handlers and own nothing.
    let dependencies: BTreeMap<String, String> = dependencies();
    let mut found: BTreeMap<String, String> = dependencies
        .iter()
        .filter(|(_, package)| crates.contains_key(*package))
        .map(|(alias, package)| (alias.clone(), package.clone()))
        .collect();
    for text in sources {
        for rest in use_statements(text) {
            if rest.starts_with("crate::")
                || rest.starts_with("super::")
                || rest.starts_with("self::")
            {
                continue;
            }
            let head = rest
                .split(|character: char| {
                    character == ':' || character == '{' || character == ' ' || character == ';'
                })
                .next()
                .unwrap_or_default();
            if head.is_empty() {
                continue;
            }
            // The crate a `use` line reaches into, by either of the two names it goes by: the package itself
            // (`use vimbai_budgeting::...`) or the alias a manifest gave it (`use budgeting_services::{...}`).
            let package = if head.starts_with("vimbai_") {
                head.to_string()
            } else {
                match dependencies.get(head) {
                    Some(package) => package.clone(),
                    None => continue,
                }
            };
            if !crates.contains_key(&package) {
                continue;
            }
            let alias = match rest.find(" as ") {
                Some(at) => rest[at + 4..]
                    .split([';', ' '])
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                None => head.trim_start_matches("vimbai_").to_string(),
            };
            if !alias.is_empty() {
                found.insert(alias, package.clone());
            }
            // A crate reached through its modules - `use budgeting_services::{master_budget, rolling_forecast}` - is called
            // as `master_budget::analyze`. Those names are things inside the crate rather than aliases of it, and without
            // them a handler that names one looks like it delegates to nothing at all: thirty-four addresses, among them
            // every budgeting calculation, were documented as unattributed for exactly that reason.
            // `use invoicing_services::invoices;` reaches a module without a brace group: the segment after the crate is
            // the thing the handler calls. A CamelCase segment is a type (`common::Money`), not a module, so it is left out.
            if !rest.contains('{') {
                if let Some((_, tail)) = rest.split_once("::") {
                    let item = tail
                        .split(|character: char| {
                            character == ':' || character == ';' || character == ' '
                        })
                        .next()
                        .unwrap_or_default();
                    if item
                        .chars()
                        .next()
                        .is_some_and(|first| first.is_lowercase())
                    {
                        found.insert(item.to_string(), package.clone());
                    }
                    if let Some(at) = rest.find(" as ") {
                        let renamed = rest[at + 4..].split([';', ' ']).next().unwrap_or_default();
                        if !renamed.is_empty() {
                            found.insert(renamed.to_string(), package.clone());
                        }
                    }
                }
            }
            if let Some(open) = rest.find('{') {
                if let Some(close) = rest[open..].find('}') {
                    for item in rest[open + 1..open + close].split(',') {
                        let item = item.trim();
                        if item.is_empty() {
                            continue;
                        }
                        let (module, renamed) = match item.find(" as ") {
                            Some(at) => (item[..at].trim(), Some(item[at + 4..].trim())),
                            None => (item, None),
                        };
                        if !module.is_empty() {
                            found.insert(module.to_string(), package.clone());
                        }
                        if let Some(renamed) = renamed.filter(|renamed| !renamed.is_empty()) {
                            found.insert(renamed.to_string(), package.clone());
                        }
                    }
                }
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
        // A handler is a wrapper: its body names the crate that does the work before it names anything else.
        //
        // Its *body*, not the rest of the file. Scanning from the handler to the end of the source found the crate of some
        // later handler whenever the wrapper itself named nothing this module knows - which is how a cache statistic that
        // reads the cache ended up filed under workflow, the next thing written below it in the same file.
        let body = body_of(&text[at..]);
        if let Some(krate) = crate_named_in(body, aliases, documented) {
            return Some(krate);
        }
    }
    None
}

/// A function's body: from its opening brace to the closing one that starts a line, which is where a top-level `fn` ends.
///
/// Counting braces would be better if the source had none inside strings, and this file's neighbours are full of
/// `format!("{key}")`. A closing brace at the start of a line is the shape every handler in this engine actually ends with.
fn body_of(text: &str) -> &str {
    let Some(start) = text.find('{') else {
        return "";
    };
    let rest = &text[start..];
    match rest.find("\n}") {
        Some(end) => &rest[..end + 1],
        None => rest,
    }
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
    let mut found: BTreeMap<String, String> = BTreeMap::new();
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
        // A page claims a prefix when the prefix is the *subject* of a line - a headline or a bullet - and not merely
        // when the page mentions it in prose: `/identity` appears in the organisation pages and belongs to neither.
        //
        // It also claims the prefixes in its address table. Every page has one, `| service | prefix |`, and it is the
        // page's own statement of what that domain answers; reading only prose left those tables unread, so a crate's
        // attribution counted for more than the page's, and a door the engine answers itself - the calculator
        // catalogue, which no domain crate answers - had no owner at all.
        for line in text.lines() {
            for cell in line.split('|') {
                let token = cell.trim().trim_matches('`').trim();
                if token.starts_with('/') && !token.contains(' ') {
                    found.entry(token.to_string()).or_insert(domain.clone());
                }
            }
            let trimmed = line
                .trim_start()
                .trim_start_matches(['|', '-', '*', '#', ' '])
                .trim_start();
            let Some(rest) = trimmed.strip_prefix('`') else {
                continue;
            };
            let token = rest.split('`').next().unwrap_or_default().trim();
            if token.starts_with('/') && !token.contains(' ') {
                found.entry(token.to_string()).or_insert(domain.clone());
            }
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
        // The page states its crates under `Crate:` or `Crates:`, one or several to a line. Reading only the singular
        // label left every page that grouped its crates unread - the platform page lists twelve that way - so their
        // addresses fell through to "whichever page happens to mention the prefix" instead of the page that covers them.
        for line in text.lines() {
            if !line.starts_with("Crate:") && !line.starts_with("Crates:") {
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
