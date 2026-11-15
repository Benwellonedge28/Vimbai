//! What `README.md` claims, checked against the things that decide it.
//!
//! The front page is the first thing a reader trusts and the least tested page in the repository: prose does not fail a build
//! when it stops being true. This module checks the claims that can be checked mechanically and leaves the rest to a reader.
//!
//! Two claims are checked today:
//!
//! 1. **How many surfaces are routed.** The number belongs to `catalog/services/catalog.yaml`, which is generated from the
//!    gateway's route table. A README figure that disagrees with it is worse than no figure, because it looks measured.
//! 2. **Which decisions it cites.** A citation like `ADR-005` sends a reader to a file, so one that does not exist is a dead
//!    end in the most prominent place. This is the rule `boundaries.rs` holds its tolerated dependency to, pointed at prose.
#![forbid(unsafe_code)]

use std::path::Path;

/// Every problem the front page has with the facts it states.
pub fn readme_claims(root: &Path) -> Vec<String> {
    let Ok(readme) = std::fs::read_to_string(root.join("README.md")) else {
        return vec!["README.md is gone, so there is no front page to check".to_string()];
    };
    let mut problems = routed_surface_count(root, &readme);
    problems.extend(adr_citations(root, &readme));
    problems
}

/// The catalogue's own count of routed services, for a caller that wants the number rather than the verdict.
pub fn routed_services(root: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(root.join("catalog/services/catalog.yaml")).ok()?;
    text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("services: ")
            .and_then(|count| count.trim().parse().ok())
    })
}

/// A sentence that puts a number beside "routed surfaces" or "routed services" is stating the catalogue's count, so it has to
/// be that count. Keeping other digits out of such a sentence is a small price for a figure that is checked.
fn routed_surface_count(root: &Path, readme: &str) -> Vec<String> {
    let Some(actual) = routed_services(root) else {
        return vec![
            "the catalogue does not state how many services it holds, so the README cannot be checked against it".to_string(),
        ];
    };
    let mut problems = Vec::new();
    for sentence in sentences(readme) {
        let lowered = sentence.to_lowercase();
        if !(lowered.contains("routed surface") || lowered.contains("routed service")) {
            continue;
        }
        // a citation in the same sentence is not a figure: `(ADR-007)` does not say seven surfaces
        for number in numbers(&without_citations(&sentence)) {
            if number != actual {
                problems.push(format!(
                    "README.md says {number} in a sentence about routed surfaces; the catalogue has {actual}"
                ));
            }
        }
    }
    problems
}

/// Every `ADR-0NN` the front page cites has to exist, under that number.
fn adr_citations(root: &Path, readme: &str) -> Vec<String> {
    let directory = root.join("docs/decisions");
    let mut existing: Vec<String> = std::fs::read_dir(&directory)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    existing.sort();
    if existing.is_empty() {
        return vec!["there are no decisions, so a citation cannot be checked".to_string()];
    }
    let mut problems = Vec::new();
    for citation in cited_adrs(readme) {
        if !existing.iter().any(|name| name.starts_with(&citation)) {
            problems.push(format!(
                "README.md cites {citation}, which is not a decision in docs/decisions"
            ));
        }
    }
    problems
}

/// The `ADR-0NN` references in a body of text, once each, in the order they first appear.
pub fn cited_adrs(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index + 4 <= bytes.len() {
        if bytes[index..].starts_with(b"ADR-") {
            let mut end = index + 4;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > index + 4 {
                let citation = format!("ADR-{}", &text[index + 4..end]);
                if !found.contains(&citation) {
                    found.push(citation);
                }
                index = end;
                continue;
            }
        }
        index += 1;
    }
    found
}

/// The sentences of a body of text, split at a full stop, question mark or exclamation mark.
fn sentences(text: &str) -> Vec<String> {
    text.split(['.', '!', '?'])
        .map(|sentence| sentence.trim().to_string())
        .filter(|sentence| !sentence.is_empty())
        .collect()
}

/// A sentence with its `ADR-0NN` citations reduced to the bare words, so a citation's digits are never read as a figure.
fn without_citations(sentence: &str) -> String {
    let mut text = String::with_capacity(sentence.len());
    let bytes = sentence.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"ADR-") {
            let mut end = index + 4;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > index + 4 {
                text.push_str("ADR");
                index = end;
                continue;
            }
        }
        text.push(bytes[index] as char);
        index += 1;
    }
    text
}

/// The numbers in a sentence.
fn numbers(sentence: &str) -> Vec<u32> {
    sentence
        .split(|character: char| !character.is_ascii_digit())
        .filter(|digits| !digits.is_empty() && digits.len() <= 4)
        .filter_map(|digits| digits.parse::<u32>().ok())
        .collect()
}

/// Every problem the roadmap's own status table has with the facts it states.
///
/// The table is the second page a reader trusts and, like the front page, it does not fail a build when it stops being true: it
/// said 139 routed services while the catalogue held 143, and 2,910 tests long after the suite had grown. Figures that can be
/// derived from the tree are derived here and compared; figures that only a run can produce are gone from the table rather than
/// stated from memory.
pub fn roadmap_claims(root: &Path) -> Vec<String> {
    let Ok(roadmap) = std::fs::read_to_string(root.join("docs/ROADMAP.md")) else {
        return vec![
            "docs/ROADMAP.md is gone, so the status table it carries cannot be checked".to_string(),
        ];
    };
    let mut problems = Vec::new();
    let mut checked = 0;
    for (label, value) in status_rows(&roadmap) {
        let wanted: Option<(&str, usize)> = match label.as_str() {
            "Routed services" => {
                routed_services(root).map(|count| ("routed services", count as usize))
            }
            "Domain crates" => Some(("domain crates", crates_under(root, "rust/crates/domains"))),
            "Calculation crates" => Some((
                "calculation crates",
                crates_under(root, "rust/crates/calculations"),
            )),
            "Platform crates" => Some((
                "platform crates",
                crates_under(root, "rust/crates/platform"),
            )),
            "Python in the repository" => Some(("python files", python_files(root))),
            "Canonical inventory" => Some(("domain files", yaml_files(root, "catalog/services"))),
            "Flutter app" => Some(("dart files", files_under(root, "apps/flutter/lib", "dart"))),
            _ => None,
        };
        let Some((described, actual)) = wanted else {
            continue;
        };
        let stated = numbers(&without_citations(&value));
        checked += 1;
        if stated.is_empty() {
            problems.push(format!(
                "docs/ROADMAP.md states the {described} with no figure, so a reader cannot tell it is current"
            ));
            continue;
        }
        for number in stated {
            if number as usize != actual {
                problems.push(format!(
                    "docs/ROADMAP.md says {number} {described}; the tree has {actual}"
                ));
            }
        }
    }
    // a label this module knows nothing about means the table was rewritten and the checks went quiet, which is the failure
    // mode worth failing on
    let wanted = 7;
    if checked < wanted {
        problems.push(format!(
            "docs/ROADMAP.md: {checked} of the {wanted} checkable facts in the status table were found; the table was rewritten"
        ));
    }
    problems
}

/// The `| Fact | Value |` rows of the roadmap's status table.
fn status_rows(roadmap: &str) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for line in roadmap.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix('|') else {
            continue;
        };
        let cells: Vec<&str> = rest.split('|').map(str::trim).collect();
        if cells.len() < 2 {
            continue;
        }
        let (label, value) = (cells[0], cells[1]);
        if label.is_empty() || label == "Fact" || label.starts_with("---") {
            continue;
        }
        rows.push((label.to_string(), value.to_string()));
    }
    rows
}

fn crates_under(root: &Path, relative: &str) -> usize {
    let Ok(entries) = std::fs::read_dir(root.join(relative)) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("Cargo.toml").is_file())
        .count()
}

fn yaml_files(root: &Path, relative: &str) -> usize {
    let Ok(entries) = std::fs::read_dir(root.join(relative)) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            let path = entry.path();
            path.extension()
                .is_some_and(|extension| extension == "yaml")
                && path.file_name().is_some_and(|name| name != "catalog.yaml")
        })
        .count()
}

fn files_under(root: &Path, relative: &str, extension: &str) -> usize {
    let mut count = 0;
    let mut queue = vec![root.join(relative)];
    while let Some(directory) = queue.pop() {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                queue.push(path);
            } else if path.extension().is_some_and(|found| found == extension) {
                count += 1;
            }
        }
    }
    count
}

fn python_files(root: &Path) -> usize {
    // `safety` already answers this question for the safe-Rust rule. Asking it again here rather than scanning twice is what
    // keeps the two gates from disagreeing about what counts as Python.
    crate::safety::python_files(root)
        .iter()
        .filter(|problem| problem.ends_with(" is Python"))
        .count()
}
