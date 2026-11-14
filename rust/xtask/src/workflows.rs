//! The workflow files, read for the two things that have actually broken.
//!
//! `test-core.yml` spent months unable to start: one shard entry opened a quoted list and never closed it, so the file
//! was not valid YAML and every push produced a run with no jobs and a failure. The tests were covered elsewhere, which
//! is exactly why nobody noticed - the only symptom was one more red run among red runs.
//!
//! That file is deleted with the last Python service directory. What survives is what it taught, plus the rule this
//! repository keeps needing: a workflow may not name something that is not there. A file that names a deleted service
//! fails when the runner reaches that line, which is why it is checked here instead.
//!
//! The files are read as text because the workspace carries no YAML parser deliberately. Two consequences are stated
//! rather than hidden: "valid YAML" is checked as "has a jobs block", and the service names are found by their suffix
//! rather than by walking a parsed matrix.
#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::path::Path;

use crate::gateway;

fn workflow_files(root: &Path) -> Vec<(String, String)> {
    let directory = root.join(".github/workflows");
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut files: Vec<(String, String)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yml" || extension == "yaml")
        })
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&path).ok()?;
            Some((name, text))
        })
        .collect();
    files.sort();
    files
}

/// The service names a workflow mentions, read from the shapes that name one: a matrix entry, a `name:` or a
/// `service:` value. Prose is not read, because "covers non-service dirs" is a comment and not a claim that
/// `non-service` exists - the first version of this scanner matched it and failed the build.
fn named_services(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for line in text.lines() {
        let trimmed = line.trim_start_matches(char::is_whitespace);
        let candidate = if let Some(rest) = trimmed.strip_prefix("- ") {
            rest
        } else if let Some((_, rest)) = trimmed.split_once("name: ") {
            rest
        } else if let Some((_, rest)) = trimmed.split_once("service: ") {
            rest
        } else {
            continue;
        };
        let candidate = candidate
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim_end_matches(',');
        if candidate.ends_with("-service")
            && candidate.chars().all(|character| {
                character.is_ascii_alphanumeric() || character == '-' || character == '_'
            })
        {
            names.insert(candidate.to_string());
        }
    }
    names
}

/// What is wrong with the workflows, in the order a person would fix it.
pub fn problems(root: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let files = workflow_files(root);
    if files.is_empty() {
        problems.push("no workflow files found, so every check below is vacuous".to_string());
        return problems;
    }
    let routed: BTreeSet<String> = gateway::entries(root)
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    for (name, text) in &files {
        if !text.contains("\njobs:") {
            problems.push(format!(
                "{name} defines no jobs, so a run of it does nothing"
            ));
        }
        for named in named_services(text) {
            if !routed.contains(&named) {
                problems.push(format!(
                    "{name} names {named}, which the gateway does not route"
                ));
            }
        }
        if text.contains("pytest") {
            problems.push(format!(
                "{name} still runs pytest, and there is no Python left to run"
            ));
        }
    }
    if files.iter().any(|(name, _)| name == "test-core.yml") {
        problems.push(
            "test-core.yml is back; it existed to run each Python service's own suite".to_string(),
        );
    }
    problems
}

/// The workflow files that exist, for a caller that wants to print them.
pub fn names(root: &Path) -> Vec<String> {
    workflow_files(root)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}
