//! Which Rust modules can anything actually reach?
//!
//! Four times a calculation was written in Rust and then never given a door: the float CVP, basis apportionment, process
//! costing, and - found by this scan - twenty more modules including IFRS 15 revenue, IFRS 2 share-based payment, IFRS 9
//! instruments, business valuation and time value of money. Every one of them looked wired, because every one of them is
//! `pub` and re-exported.
//!
//! A module is reachable when it is imported, directly or through another crate's module, by one of the two doors the
//! platform has left: the API server and the adapters it calls. Nothing else can reach the library, so a module nothing
//! reaches is arithmetic no request can execute.
//!
//! Two details decide whether the answer is true. A dependency is often imported by a shorter name than its package
//! (`use assets::{depreciation}` names the directory, the package is `vimbai_assets`), so the alias map is read from each
//! Cargo.toml. And a door that calls `assets::amortization_from_json` never names the module: the answer is the `pub use`
//! line in `lib.rs`. Resolving that by "which module happens to define the name" read a different verdict on different
//! runs.
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

/// The doors: code the platform actually starts.
const DOORS: [&str; 2] = ["rust/apps/", "rust/crates/adapters/"];

type Node = (String, Vec<String>);

/// Every offset at which `needle` starts in `haystack`.
fn find_all(haystack: &str, needle: &str) -> Vec<usize> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut offsets = Vec::new();
    let mut from = 0usize;
    while let Some(found) = haystack[from..].find(needle) {
        offsets.push(from + found);
        from += found + needle.len();
    }
    offsets
}

#[derive(Default)]
struct Graph {
    files: Vec<PathBuf>,
    nodes: BTreeMap<PathBuf, Node>,
    edges: BTreeMap<Node, BTreeSet<Node>>,
}

fn package_name(manifest: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(manifest) else {
        return String::new();
    };
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("name = \"") {
            if let Some(end) = rest.find('"') {
                return rest[..end].to_string();
            }
        }
    }
    String::new()
}

fn crate_dir(root: &Path, file: &Path) -> PathBuf {
    let mut directory = file.parent().unwrap_or(file).to_path_buf();
    loop {
        if directory.join("Cargo.toml").is_file() {
            return directory;
        }
        match directory.parent() {
            Some(parent) if parent.starts_with(root) => directory = parent.to_path_buf(),
            _ => return directory,
        }
    }
}

fn relative(directory: &Path, root: &Path) -> String {
    directory
        .strip_prefix(root)
        .unwrap_or(directory)
        .to_string_lossy()
        .replace('\\', "/")
}

fn rust_files(directory: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            rust_files(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            into.push(path);
        }
    }
}

/// Dependency name -> package name for one crate, as its own Cargo.toml declares them.
fn aliases(
    directory: &Path,
    packages: &BTreeMap<PathBuf, String>,
    root: &Path,
) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(directory.join("Cargo.toml")) else {
        return found;
    };
    let mut in_dependencies = false;
    for line in text.lines() {
        let stripped = line.split('#').next().unwrap_or("").trim();
        if stripped.starts_with('[') {
            let section = stripped.trim_matches(&['[', ']'][..]);
            in_dependencies = section == "dependencies"
                || section == "dev-dependencies"
                || section == "build-dependencies"
                || section.starts_with("dependencies.");
            continue;
        }
        if !in_dependencies {
            continue;
        }
        let Some((key, rest)) = stripped.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty()
            || !key
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            continue;
        }
        let alias = key.replace('-', "_");
        if let Some(start) = rest.find("package") {
            let after = &rest[start..];
            if let Some(open) = after.find('"') {
                if let Some(end) = after[open + 1..].find('"') {
                    found.insert(alias, after[open + 1..open + 1 + end].to_string());
                    continue;
                }
            }
        }
        if let Some(start) = rest.find("path") {
            let after = &rest[start..];
            if let Some(open) = after.find('"') {
                if let Some(end) = after[open + 1..].find('"') {
                    let target = directory.join(&after[open + 1..open + 1 + end]);
                    let normalised = target
                        .components()
                        .collect::<PathBuf>()
                        .to_string_lossy()
                        .replace('\\', "/");
                    let resolved = packages
                        .iter()
                        .find(|(directory, _)| {
                            relative(directory, root) == normalised.trim_start_matches("./")
                        })
                        .map(|(_, package)| package.clone());
                    if let Some(package) = resolved {
                        found.insert(alias, package);
                        continue;
                    }
                }
            }
        }
        found.insert(alias.clone(), key.to_string());
    }
    found
}

fn module_path(file: &Path, directory: &Path) -> Vec<String> {
    let source = directory.join("src");
    let relative = file
        .strip_prefix(&source)
        .unwrap_or(file)
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/");
    if relative == "lib" {
        return Vec::new();
    }
    relative.split('/').map(str::to_string).collect()
}

fn build(root: &Path) -> Graph {
    let rust = root.join("rust");
    let mut files = Vec::new();
    rust_files(&rust, &mut files);
    files.sort();
    let mut graph = Graph {
        files: files.clone(),
        ..Graph::default()
    };
    let mut crate_names: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut packages: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut manifests = Vec::new();
    rust_files(&rust, &mut manifests);
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
                packages.insert(directory.clone(), package_name(&path));
            }
        }
    }
    for file in &files {
        let directory = crate_dir(&rust, file);
        let name = crate_names
            .entry(directory.clone())
            .or_insert_with(|| package_name(&directory.join("Cargo.toml")))
            .clone();
        graph
            .nodes
            .insert(file.clone(), (name, module_path(file, &directory)));
    }
    for file in &files {
        let directory = crate_dir(&rust, file);
        let node = graph.nodes[file].clone();
        let Ok(source) = std::fs::read_to_string(file) else {
            continue;
        };
        let mut edges = BTreeSet::new();
        for (alias, package) in aliases(&directory, &packages, &rust) {
            let marker = format!("{alias}::");
            for at in find_all(&source, &marker) {
                if at > 0 {
                    let before = source[..at].chars().next_back().unwrap_or(' ');
                    if before.is_ascii_alphanumeric() || before == '_' {
                        continue;
                    }
                }
                let rest = &source[at + marker.len()..];
                let path = leading_module_path(rest);
                if path.is_empty() {
                    continue;
                }
                edges.insert((package.clone(), path));
            }
            // `use treasury::{cash_optimization, treasury_analytics};` hides every module behind the brace.
            let group = format!("{alias}::{{");
            for at in find_all(&source, &group) {
                let rest = &source[at + group.len()..];
                let Some(end) = rest.find('}') else {
                    continue;
                };
                for item in rest[..end].split(',') {
                    let head = item.trim().split("::").next().unwrap_or("").trim();
                    if !head.is_empty() && head != "self" && is_module_segment(head) {
                        edges.insert((package.clone(), vec![head.to_string()]));
                    }
                }
            }
        }
        for at in find_all(&source, "crate::") {
            let path = leading_module_path(&source[at + "crate::".len()..]);
            if !path.is_empty() {
                edges.insert((node.0.clone(), path));
            }
        }
        for at in find_all(&source, "super::") {
            let mut parent = node.1.clone();
            parent.pop();
            let rest = &source[at + "super::".len()..];
            let segment = leading_segment(rest);
            if !segment.is_empty() {
                let mut target = parent;
                target.push(segment);
                edges.insert((node.0.clone(), target));
            }
        }
        for at in find_all(&source, "vimbai_") {
            let rest = &source[at + "vimbai_".len()..];
            let Some(end) = rest.find("::") else {
                continue;
            };
            let crate_suffix = &rest[..end];
            if crate_suffix.is_empty()
                || !crate_suffix
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                continue;
            }
            let path = leading_module_path(&rest[end + 2..]);
            if !path.is_empty() {
                edges.insert((format!("vimbai_{crate_suffix}"), path));
            }
        }
        graph.edges.insert(node, edges);
    }
    graph
}

fn is_module_segment(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn leading_segment(text: &str) -> String {
    text.chars()
        .take_while(|c| is_module_segment(&c.to_string()))
        .collect()
}

/// `foo::bar::baz` -> ["foo", "bar", "baz"], stopping at anything that is not a module segment.
fn leading_module_path(text: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        if is_module_segment(&character.to_string()) {
            current.push(character);
            continue;
        }
        break;
    }
    if !current.is_empty() {
        segments.push(current);
    }
    while segments.last().is_some_and(|_| text.contains("::")) {
        let consumed: usize = segments.iter().map(|segment| segment.len()).sum::<usize>()
            + segments.len().saturating_sub(1) * 2;
        let rest = &text[consumed..];
        let Some(rest) = rest.strip_prefix("::") else {
            break;
        };
        let next: String = rest
            .chars()
            .take_while(|c| is_module_segment(&c.to_string()))
            .collect();
        if next.is_empty() {
            break;
        }
        segments.push(next);
        if segments.len() > 8 {
            break;
        }
    }
    segments
}

fn crate_roots(graph: &Graph) -> BTreeMap<String, BTreeSet<Vec<String>>> {
    let mut roots: BTreeMap<String, BTreeSet<Vec<String>>> = BTreeMap::new();
    for (package, module) in graph.nodes.values() {
        if let Some(head) = module.first() {
            roots
                .entry(package.clone())
                .or_default()
                .insert(vec![head.clone()]);
        }
    }
    roots
}

/// Which module a crate-root re-export names, which is how a door reaches a module it never mentions.
fn reexports(graph: &Graph) -> BTreeMap<(String, String), Vec<String>> {
    let mut found = BTreeMap::new();
    for (file, (package, module)) in &graph.nodes {
        if !module.is_empty() {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(file) else {
            continue;
        };
        let marker = "pub use ";
        for at in find_all(&source, marker) {
            let rest = &source[at + marker.len()..];
            let head = leading_segment(rest);
            if head.is_empty() {
                continue;
            }
            if let Some(group) = rest
                .strip_prefix(&head)
                .and_then(|tail| tail.strip_prefix("::{"))
            {
                let Some(end) = group.find('}') else {
                    continue;
                };
                for item in group[..end].split(',') {
                    let name = item
                        .trim()
                        .split(" as ")
                        .last()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if name.is_empty() {
                        continue;
                    }
                    found
                        .entry((package.clone(), name))
                        .or_insert_with(|| vec![head.clone()]);
                }
            } else if let Some(tail) = rest
                .strip_prefix(&head)
                .and_then(|tail| tail.strip_prefix("::"))
            {
                let name = leading_segment(tail);
                if !name.is_empty() {
                    found
                        .entry((package.clone(), name))
                        .or_insert_with(|| vec![head.clone()]);
                }
            }
        }
    }
    found
}

/// Where a plain re-export's name is defined, for the doors that call it with no module of their own.
fn definitions(graph: &Graph) -> BTreeMap<(String, String), BTreeSet<Vec<String>>> {
    let mut found: BTreeMap<(String, String), BTreeSet<Vec<String>>> = BTreeMap::new();
    for (file, (package, module)) in &graph.nodes {
        let Ok(source) = std::fs::read_to_string(file) else {
            continue;
        };
        for line in source.lines() {
            let Some(rest) = line.strip_prefix("pub fn ") else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            found
                .entry((package.clone(), name))
                .or_default()
                .insert(module.clone());
        }
    }
    found
}

/// The files a door can reach, following imports through as many crates as it takes.
pub fn reachable(root: &Path) -> BTreeSet<PathBuf> {
    let graph = build(root);
    let roots = crate_roots(&graph);
    let re_exported = reexports(&graph);
    let defines = definitions(&graph);
    let mut by_node: BTreeMap<Node, Vec<PathBuf>> = BTreeMap::new();
    for (file, node) in &graph.nodes {
        by_node.entry(node.clone()).or_default().push(file.clone());
    }
    let mut queue: VecDeque<Node> = graph
        .files
        .iter()
        .filter(|file| {
            let relative = relative(file, root);
            DOORS.iter().any(|door| relative.starts_with(door))
        })
        .filter_map(|file| graph.nodes.get(file).cloned())
        .collect();
    let mut seen: BTreeSet<Node> = BTreeSet::new();
    while let Some(node) = queue.pop_front() {
        if !seen.insert(node.clone()) {
            continue;
        }
        let Some(edges) = graph.edges.get(&node) else {
            continue;
        };
        for (package, module) in edges {
            let mut candidates: Vec<Vec<String>> = vec![module.clone()];
            let is_root = module.first().is_some_and(|head| {
                roots
                    .get(package)
                    .is_some_and(|set| set.contains(&vec![head.clone()]))
            });
            if !module.is_empty() && !is_root {
                if let Some(head) = module.first() {
                    if let Some(target) = re_exported.get(&(package.clone(), head.clone())) {
                        let mut expanded = target.clone();
                        expanded.extend(module[1..].iter().cloned());
                        candidates = vec![expanded];
                    } else if let Some(sites) = defines.get(&(package.clone(), head.clone())) {
                        candidates = sites.iter().cloned().collect();
                    }
                }
            }
            for candidate in candidates {
                for (known, files) in &by_node {
                    if &known.0 != package {
                        continue;
                    }
                    if candidate.len() >= known.1.len() && candidate[..known.1.len()] == known.1[..]
                    {
                        for file in files {
                            if let Some(node) = graph.nodes.get(file) {
                                queue.push_back(node.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    seen.iter()
        .flat_map(|node| by_node.get(node).cloned().unwrap_or_default())
        .collect()
}

/// Module files no door reaches, as `crate::module` strings.
pub fn unreachable_modules(root: &Path) -> Vec<String> {
    let graph = build(root);
    let reached = reachable(root);
    let crates = root.join("rust/crates");
    let mut dead = BTreeSet::new();
    for file in &graph.files {
        if reached.contains(file) || !file.starts_with(&crates) {
            continue;
        }
        let relative = relative(file, root);
        if relative.ends_with("lib.rs")
            || relative.contains("/tests/")
            || relative.ends_with("tests.rs")
        {
            continue;
        }
        let (package, module) = &graph.nodes[file];
        let module = if module.is_empty() {
            "lib".to_string()
        } else {
            module.join("::")
        };
        dead.insert(format!("{}::{module}", package.replace("vimbai_", "")));
    }
    dead.into_iter().collect()
}
