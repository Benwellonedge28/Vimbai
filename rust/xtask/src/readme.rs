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
