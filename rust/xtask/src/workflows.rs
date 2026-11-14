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

/// The Dart toolchain the app is built and tested with, as the workflows declare it.
///
/// The client is one of the three languages of the platform (ADR-001), so its toolchain belongs in the pipeline that gates a
/// merge rather than in a workflow that only runs when somebody touches the app. Two things follow, and both are checked
/// here: the gating pipeline has to actually run the app's tests, and the Flutter version has to be pinned and the same in
/// every workflow - a `latest` that drifts is a green build that means something different tomorrow.
pub fn dart_toolchain_problems(root: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let files = workflow_files(root);
    let mut versions: Vec<(String, String)> = Vec::new();
    let mut set_up_in: Vec<String> = Vec::new();
    for (name, text) in &files {
        let mut lines = text.lines();
        while let Some(line) = lines.next() {
            if !line.contains("setup-flutter") {
                continue;
            }
            set_up_in.push(name.clone());
            // the version sits in the `with:` block below the action, which is where a pinned toolchain is written
            let mut pinned: Option<String> = None;
            for following in lines.by_ref() {
                let trimmed = following.trim();
                // a comment or a blank line inside the block is not the end of it: the workflows explain their pins
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if let Some(rest) = trimmed.strip_prefix("version:") {
                    pinned = Some(rest.trim().trim_matches('"').trim_matches('\'').to_string());
                }
                // the block ends at the first line that is not more indented than the action's own step
                if !trimmed.is_empty()
                    && !trimmed.starts_with("version:")
                    && !trimmed.starts_with("channel:")
                    && !trimmed.starts_with("cache")
                    && !trimmed.starts_with("with:")
                {
                    break;
                }
            }
            match pinned {
                Some(version) if version == "latest" || version.is_empty() => problems.push(format!(
                    "{name} installs `latest` Flutter, so the toolchain changes under the build without a commit"
                )),
                Some(version) => versions.push((name.clone(), version)),
                None => problems.push(format!(
                    "{name} sets up Flutter without pinning a version"
                )),
            }
        }
    }
    if set_up_in.is_empty() {
        problems.push(
            "no workflow installs a Dart toolchain, so the app is never built or tested in CI"
                .to_string(),
        );
        return problems;
    }
    if !set_up_in.iter().any(|name| name == GATING_PIPELINE) {
        problems.push(format!(
            "the Dart toolchain is installed only outside {GATING_PIPELINE}, so a broken app does not fail a merge"
        ));
    }
    if let Some((first_workflow, first_version)) = versions.first() {
        for (name, version) in &versions {
            if version != first_version {
                problems.push(format!(
                    "{name} pins Flutter {version} while {first_workflow} pins {first_version}, so two CI runs disagree \
                     about the toolchain"
                ));
            }
        }
        if let Some((_, gating)) = files.iter().find(|(name, _)| name == GATING_PIPELINE) {
            if !gating.contains("flutter test") {
                problems.push(format!(
                    "{GATING_PIPELINE} installs a Dart toolchain and never runs `flutter test`, so the app's tests gate \
                     nothing"
                ));
            }
            if !gating.contains("dart format") {
                problems.push(format!(
                    "{GATING_PIPELINE} never checks Dart formatting, so a formatted codebase is a matter of taste"
                ));
            }
        }
    }
    problems
}

/// The workflow that gates a merge.
const GATING_PIPELINE: &str = "ci-main.yml";
