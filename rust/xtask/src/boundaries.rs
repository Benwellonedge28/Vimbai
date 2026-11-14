//! The rules between crates, and the gate that holds them.
//!
//! The Python estate had no dependency rules because it had no dependencies: each service was a container that could
//! import anything, so a calculation that needed a business rule copied the rule and the copies drifted. Porting them
//! into one workspace makes coupling possible, which means it has to be bounded on purpose rather than by habit.
//!
//! This module is written to be reused rather than to be run once: [`graph`] returns the crate dependency graph for
//! anybody who wants it (the contract gate and later phases do), [`rules`] is the table of what each family may depend
//! on, and [`violations`] applies the table. Adding a rule is adding a row; adding a crate is nothing at all, because
//! the graph is discovered from the manifests instead of listed here.
//!
//! Manifests are read by their line shape. The workspace carries no TOML parser on purpose - it would add a dependency
//! to the one tool that has to build on a machine with no network - so a scan that finds no crates fails rather than
//! passing over an empty set.
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// What a crate is, which is what decides what it may depend on.
///
/// The family comes from where the crate lives, not from its name: a directory is harder to get wrong than a naming
/// convention, and a crate that lands outside every family is a violation in its own right rather than silently
/// unclassified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Family {
    /// Shared vocabulary: money, identifiers, hashing, dates. The bottom of the graph.
    Common,
    /// The ledger kernel. Owns posting integrity and nothing else.
    Kernel,
    /// Arithmetic with no state and no store: amounts in, amounts out.
    Calculations,
    /// Infrastructure the platform provides to everything: identity, access, events, documents, storage plumbing.
    Platform,
    /// Business rules and orchestration for one domain of the business.
    Domain,
    /// Drivers and seams: the property store, the JSON wire adapter.
    Adapters,
    /// Use cases that compose domains for a caller.
    Application,
    /// Binaries: the API server, the on-device runtime, and whatever follows them.
    /// A suite: the integrity tests that sit outside every crate and check what the ledger promises. It may depend on
    /// anything, because it is the audit, and nothing may depend on it, because an audit that the code needs is not an
    /// audit.
    Suite,
    Apps,
    /// Repository tooling that reads the tree. Must stay buildable on its own.
    Tooling,
}

impl Family {
    /// The name used in reports and in `docs/architecture/dependency-rules.md`.
    pub fn label(self) -> &'static str {
        match self {
            Family::Common => "common",
            Family::Kernel => "kernel",
            Family::Calculations => "calculations",
            Family::Platform => "platform",
            Family::Domain => "domain",
            Family::Adapters => "adapters",
            Family::Application => "application",
            Family::Apps => "apps",
            Family::Suite => "suite",
            Family::Tooling => "tooling",
        }
    }
}

/// One crate: where it lives, what it is, and which workspace crates it names.
#[derive(Debug, Clone)]
pub struct Crate {
    /// The package name, which is what another manifest depends on.
    pub name: String,
    /// Path relative to the repository root, for a report a human can act on.
    pub path: String,
    /// `None` when the crate lives somewhere no family claims.
    pub family: Option<Family>,
    /// Workspace crates named by `[dependencies]` and `[build-dependencies]`.
    pub dependencies: BTreeSet<String>,
    /// Workspace crates named only by `[dev-dependencies]`. Reported, not ruled on: a test may reach for a fixture
    /// that the code it tests must not reach for at runtime.
    pub dev_dependencies: BTreeSet<String>,
}

impl Crate {
    pub fn is(&self, family: Family) -> bool {
        self.family == Some(family)
    }
}

/// A rule: crates in `from` may depend only on crates in `allowed`.
#[derive(Debug, Clone, Copy)]
pub struct Rule {
    /// Short identifier, quoted in the documentation so the two cannot drift apart silently.
    pub id: &'static str,
    /// The family the rule constrains.
    pub from: Family,
    /// The families a crate in `from` may depend on, itself included when that is intended.
    pub allowed: &'static [Family],
    /// The rule in one sentence, as it appears in the documentation.
    pub statement: &'static str,
}

const RULES: [Rule; 9] = [
    Rule {
        id: "R1",
        from: Family::Common,
        allowed: &[],
        statement: "common depends on no other workspace crate; it is the vocabulary everything else shares.",
    },
    Rule {
        id: "R2",
        from: Family::Kernel,
        allowed: &[Family::Common, Family::Kernel],
        statement: "kernel may depend on common and other kernel crates only, so the ledger authority cannot borrow a \
                    business rule.",
    },
    Rule {
        id: "R3",
        from: Family::Calculations,
        allowed: &[Family::Common, Family::Kernel, Family::Calculations],
        statement: "calculations may depend on common, kernel and other calculations; never on a domain, a platform \
                    crate, an adapter or an app.",
    },
    Rule {
        id: "R4",
        from: Family::Platform,
        allowed: &[
            Family::Common,
            Family::Kernel,
            Family::Platform,
            Family::Adapters,
        ],
        statement: "platform may depend on common, kernel, other platform crates and the adapters, never on a domain.",
    },
    Rule {
        id: "R5",
        from: Family::Domain,
        allowed: &[
            Family::Common,
            Family::Kernel,
            Family::Calculations,
            Family::Platform,
            Family::Adapters,
        ],
        statement: "a domain may depend on common, kernel, calculations, platform and the adapters, but not on another \
                    domain: two domains talk through a published surface, not by importing each other.",
    },
    Rule {
        id: "R6",
        from: Family::Adapters,
        allowed: &[
            Family::Common,
            Family::Kernel,
            Family::Platform,
            Family::Calculations,
            Family::Domain,
            Family::Adapters,
        ],
        statement: "adapters sit at the edge and may reach anything below them; the property store's own purity is \
                    enforced by the store rule in crates.rs rather than repeated here.",
    },
    Rule {
        id: "R7",
        from: Family::Application,
        allowed: &[
            Family::Common,
            Family::Kernel,
            Family::Calculations,
            Family::Platform,
            Family::Domain,
            Family::Adapters,
            Family::Application,
        ],
        statement: "application composes domains, calculations, platform and the adapters.",
    },
    Rule {
        id: "R9",
        from: Family::Suite,
        allowed: &[
            Family::Common,
            Family::Kernel,
            Family::Calculations,
            Family::Platform,
            Family::Domain,
            Family::Adapters,
            Family::Application,
            Family::Apps,
            Family::Suite,
        ],
        statement: "a suite may depend on any crate in the workspace, because its job is to call the published surface \
                    from outside; nothing may depend on a suite.",
    },
    Rule {
        id: "R8",
        from: Family::Tooling,
        allowed: &[],
        statement: "tooling depends on no workspace crate, so the gates build before the workspace does and cannot be \
                    made to agree with the code by the code.",
    },
];

/// A dependency the rules forbid that is nonetheless deliberate, with the reason it is tolerated.
///
/// The point of keeping these in code rather than in a document is that they are checked twice: the dependency has to
/// still exist ([`stale_exceptions`] fails if it does not, so nobody leaves a dead exception behind), and the
/// documentation has to name it. Tolerating a dependency without a reason is what the rules exist to prevent.
#[derive(Debug, Clone, Copy)]
pub struct Exception {
    /// The rule it would otherwise break.
    pub rule: &'static str,
    /// The crate that depends.
    pub from: &'static str,
    /// The crate depended on.
    pub to: &'static str,
    /// Why, in one sentence a reader can disagree with.
    pub reason: &'static str,
}

const EXCEPTIONS: [Exception; 1] = [Exception {
    rule: "R2",
    from: "vimbai_ledger_services",
    to: "vimbai_store",
    reason: "The kernel's reading half reconciles control accounts and reports integrity by reading the ledger back out \
             of the store. The posting authority, vimbai-ledger-core, stays pure and store-free, which is the guarantee \
             that matters; reading is not posting.",
}];

/// The tolerated dependencies, for a report or a test to read.
pub fn exceptions() -> &'static [Exception] {
    &EXCEPTIONS
}

/// The rule table, for a generator or a report to read.
pub fn rules() -> &'static [Rule] {
    &RULES
}

/// The rule constraining one family, if it has one.
pub fn rule_for(family: Family) -> Option<&'static Rule> {
    RULES.iter().find(|rule| rule.from == family)
}

fn repository_of(path: &Path) -> PathBuf {
    let text = path.to_string_lossy().replace('\\', "/");
    text.strip_suffix("/Cargo.toml")
        .map(PathBuf::from)
        .unwrap_or_else(|| path.to_path_buf())
}

/// Map a path below `rust/` such as `crates/domains/costing` to its family.
fn family_from_parts(relative: &str) -> Option<Family> {
    let trimmed = relative.trim_start_matches("rust/");
    if trimmed == "xtask" {
        return Some(Family::Tooling);
    }
    if trimmed.starts_with("apps/") {
        return Some(Family::Apps);
    }
    if trimmed.starts_with("tests/") {
        return Some(Family::Suite);
    }
    if trimmed == "crates/common" {
        return Some(Family::Common);
    }
    if trimmed == "crates/application" {
        return Some(Family::Application);
    }
    let rest = trimmed.strip_prefix("crates/")?;
    let (group, _) = rest.split_once('/')?;
    match group {
        "kernel" => Some(Family::Kernel),
        "calculations" => Some(Family::Calculations),
        "platform" => Some(Family::Platform),
        "domains" => Some(Family::Domain),
        "adapters" => Some(Family::Adapters),
        _ => None,
    }
}

/// The package name a manifest declares.
pub fn package_name(manifest: &str) -> Option<String> {
    for line in manifest.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("name = \"") {
            return rest.split('"').next().map(str::to_string);
        }
        // A `name = "..."` under `[[bin]]` must not be mistaken for the package, so stop at the first table header
        // that is not `[package]`.
        if trimmed.starts_with('[') && trimmed != "[package]" && trimmed != "[package.metadata]" {
            return None;
        }
    }
    None
}

/// Workspace crates named by a manifest, split into runtime and dev.
///
/// Both `foo = { path = "..." }` and `foo = { package = "bar", path = "..." }` are read, because a rename in one
/// manifest is the same dependency as far as the graph is concerned.
fn declared_dependencies(
    manifest: &str,
    known: &BTreeSet<String>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut runtime = BTreeSet::new();
    let mut dev = BTreeSet::new();
    let mut section = String::new();
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            section = trimmed.trim_matches(&['[', ']'][..]).to_string();
            // A dependency declared as its own table still counts, and a target-specific table is the same
            // dependency as far as the graph is concerned.
            if let Some(name) = section.strip_prefix("dependencies.") {
                consider(name, &mut runtime, known);
            } else if let Some(name) = section.strip_prefix("dev-dependencies.") {
                consider(name, &mut dev, known);
            } else if let Some((_, name)) = section.rsplit_once(".dependencies.") {
                consider(name, &mut runtime, known);
            } else if let Some((_, name)) = section.rsplit_once(".dev-dependencies.") {
                consider(name, &mut dev, known);
            }
            continue;
        }
        let target = match section.as_str() {
            "dependencies" | "build-dependencies" => &mut runtime,
            "dev-dependencies" => &mut dev,
            other if other.ends_with(".dependencies") => &mut runtime,
            other if other.ends_with(".dev-dependencies") => &mut dev,
            _ => continue,
        };
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim().trim_matches('"');
        if !value.contains("path") && !value.contains("workspace") {
            continue;
        }
        if let Some(package) = package_alias(value) {
            if known.contains(&package) {
                target.insert(package);
            }
            continue;
        }
        if known.contains(key) {
            target.insert(key.to_string());
        }
    }
    (runtime, dev)
}

fn consider(name: &str, into: &mut BTreeSet<String>, known: &BTreeSet<String>) {
    if known.contains(name) {
        into.insert(name.to_string());
    }
}

fn package_alias(value: &str) -> Option<String> {
    let start = value.find("package = \"")? + "package = \"".len();
    value[start..].split('"').next().map(str::to_string)
}

/// Every crate in the workspace, with the workspace crates it depends on.
///
/// `vimbai_store`'s manifest is named by other crates as the store; its package name is what appears as a dependency.
pub fn graph(root: &Path) -> Vec<Crate> {
    let rust = root.join("rust");
    let mut manifests: Vec<PathBuf> = Vec::new();
    let mut stack = vec![rust.clone()];
    while let Some(directory) = stack.pop() {
        if directory.file_name().is_some_and(|name| name == "target") {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|name| name == "Cargo.toml") {
                manifests.push(path);
            }
        }
    }
    manifests.sort();

    let mut names = BTreeSet::new();
    let mut parsed: Vec<(PathBuf, String, String)> = Vec::new();
    for manifest in &manifests {
        let text = std::fs::read_to_string(manifest)
            .unwrap_or_else(|problem| panic!("{} is unreadable: {problem}", manifest.display()));
        let Some(name) = package_name(&text) else {
            continue;
        };
        names.insert(name.clone());
        parsed.push((manifest.clone(), name, text));
    }

    let mut crates = Vec::new();
    for (manifest, name, text) in parsed {
        let directory = repository_of(&manifest);
        let relative = directory
            .strip_prefix(&rust)
            .map(|rest| format!("rust/{}", rest.to_string_lossy()))
            .unwrap_or_else(|_| directory.to_string_lossy().into_owned());
        let (dependencies, dev_dependencies) = declared_dependencies(&text, &names);
        let mut dependencies = dependencies;
        dependencies.remove(&name);
        let mut dev_dependencies = dev_dependencies;
        dev_dependencies.remove(&name);
        dev_dependencies = dev_dependencies
            .difference(&dependencies)
            .cloned()
            .collect();
        crates.push(Crate {
            family: family_from_parts(&relative),
            name,
            path: relative,
            dependencies,
            dev_dependencies,
        });
    }
    crates.sort_by(|left, right| left.name.cmp(&right.name));
    crates
}

/// A dependency the rules forbid.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Violation {
    pub rule: &'static str,
    pub from: String,
    pub from_family: Family,
    pub to: String,
    pub to_family: Family,
    pub statement: &'static str,
}

impl Violation {
    pub fn report(&self) -> String {
        format!(
            "{}: {} ({}) depends on {} ({}) - {}",
            self.rule,
            self.from,
            self.from_family.label(),
            self.to,
            self.to_family.label(),
            self.statement
        )
    }
}

/// Crates on the wrong side of a rule.
pub fn violations_of(crates: &[Crate]) -> Vec<Violation> {
    let by_name: BTreeMap<&str, &Crate> = crates
        .iter()
        .map(|item| (item.name.as_str(), item))
        .collect();
    let mut found = Vec::new();
    for item in crates {
        let (Some(from_family), Some(rule)) = (item.family, item.family.and_then(rule_for)) else {
            continue;
        };
        for dependency in &item.dependencies {
            let Some(target) = by_name.get(dependency.as_str()) else {
                continue;
            };
            let Some(to_family) = target.family else {
                continue;
            };
            if rule.allowed.contains(&to_family) {
                continue;
            }
            if EXCEPTIONS.iter().any(|exception| {
                exception.rule == rule.id
                    && exception.from == item.name
                    && exception.to == target.name
            }) {
                continue;
            }
            found.push(Violation {
                rule: rule.id,
                from: item.name.clone(),
                from_family,
                to: target.name.clone(),
                to_family,
                statement: rule.statement,
            });
        }
    }
    found.sort();
    found
}

/// Convenience for a gate: read the tree and apply the rules.
pub fn violations(root: &Path) -> Vec<Violation> {
    violations_of(&graph(root))
}

/// Exceptions that no longer describe a real dependency, so the tolerance has outlived its reason.
pub fn stale_exceptions(crates: &[Crate]) -> Vec<String> {
    let by_name: BTreeMap<&str, &Crate> = crates
        .iter()
        .map(|item| (item.name.as_str(), item))
        .collect();
    let mut stale = Vec::new();
    for exception in EXCEPTIONS {
        let still_there = by_name
            .get(exception.from)
            .is_some_and(|item| item.dependencies.contains(exception.to));
        if !still_there {
            stale.push(format!(
                "{}: {} -> {} is recorded as tolerated but the dependency is gone; delete the exception",
                exception.rule, exception.from, exception.to
            ));
        }
    }
    stale.sort();
    stale
}

/// Crates living where no family claims them, so no rule applies.
pub fn unclassified(crates: &[Crate]) -> Vec<String> {
    crates
        .iter()
        .filter(|item| item.family.is_none())
        .map(|item| format!("{} at {}", item.name, item.path))
        .collect()
}

/// Crates that depend on a binary, which would make a library part of a program.
pub fn depending_on_an_app(crates: &[Crate]) -> Vec<String> {
    let apps: BTreeSet<&str> = crates
        .iter()
        .filter(|item| item.is(Family::Apps))
        .map(|item| item.name.as_str())
        .collect();
    let mut found = Vec::new();
    for item in crates {
        if item.is(Family::Apps) {
            continue;
        }
        for dependency in &item.dependencies {
            if apps.contains(dependency.as_str()) {
                found.push(format!("{} depends on {}", item.name, dependency));
            }
        }
    }
    found.sort();
    found
}

/// The functions that decide posting integrity, defined outside the kernel that owns them.
///
/// A dependency rule cannot see this: the copies that did the damage were local functions that happened to answer the
/// same question. The scan looks for a definition, not a call, because calling the kernel is the point.
pub fn ledger_authority_outside_the_kernel(root: &Path) -> Vec<String> {
    const AUTHORITY: [&str; 4] = [
        "fn validate_money_lines",
        "fn validate_lines",
        "fn compute_entry_hash",
        "fn check_reversal",
    ];
    let mut found = Vec::new();
    for item in graph(root) {
        if item.is(Family::Kernel) {
            continue;
        }
        let source = root.join(&item.path).join("src");
        let mut stack = vec![source];
        while let Some(directory) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|extension| extension != "rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                for line in text.lines() {
                    let trimmed = line.trim_start();
                    let trimmed = trimmed.strip_prefix("pub ").unwrap_or(trimmed);
                    for name in AUTHORITY {
                        if trimmed.starts_with(name) {
                            found.push(format!(
                                "{} defines `{}` at {}",
                                item.name,
                                name.trim_start_matches("fn "),
                                path.display()
                            ));
                        }
                    }
                }
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// A one line summary, for a CLI that prints rather than asserts.
pub fn summary(crates: &[Crate]) -> String {
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for item in crates {
        *counts
            .entry(item.family.map(Family::label).unwrap_or("unclassified"))
            .or_default() += 1;
    }
    let mut parts = Vec::new();
    for family in [
        Family::Common,
        Family::Kernel,
        Family::Calculations,
        Family::Platform,
        Family::Domain,
        Family::Adapters,
        Family::Application,
        Family::Apps,
        Family::Tooling,
    ] {
        if let Some(count) = counts.get(family.label()) {
            parts.push(format!("{} {count}", family.label()));
        }
    }
    let edges: usize = crates.iter().map(|item| item.dependencies.len()).sum();
    format!(
        "{} crates ({}), {edges} internal dependencies",
        crates.len(),
        parts.join(", ")
    )
}
