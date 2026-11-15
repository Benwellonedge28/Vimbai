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
    // One place. A second workflow that installs the toolchain runs the same four steps on the same version - which is
    // what `mobile-analyze.yml` was - and gives the pin and the steps a second place to drift out of step with each other.
    if set_up_in.len() > 1 {
        problems.push(format!(
            "{} workflows install a Dart toolchain ({}), so the app is gated in more than one place and the copies can \
             drift",
            set_up_in.len(),
            set_up_in.join(", ")
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
            // And it has to start. A gating pipeline that installs the toolchain but declares `paths:` that never match the
            // app is worse than none: the job exists, is green in the history, and no app change ever reaches it.
            if let Some(filter) = trigger_paths(gating) {
                if !filter.iter().any(|path| covers_the_app(path)) {
                    problems.push(format!(
                        "{GATING_PIPELINE} installs a Dart toolchain but only starts for {}, so an app change never reaches \
                         it",
                        filter.join(", ")
                    ));
                }
            }
        }
    }
    problems
}

/// The `paths:` entries a workflow declares, in the order they appear, with the quotes stripped.
///
/// Only the trigger block is read: a `paths:` inside a step is something else entirely. An empty result means the workflow
/// declares no path filter at all, which is the strong case - it starts for everything.
fn trigger_paths(text: &str) -> Option<Vec<String>> {
    let mut paths = Vec::new();
    let mut in_trigger = false;
    let mut reading = false;
    for line in text.lines() {
        let trimmed = line.trim_end();
        match trimmed {
            "on:" => {
                in_trigger = true;
                continue;
            }
            "jobs:" => break,
            _ => {}
        }
        if !in_trigger {
            continue;
        }
        let indent = trimmed.len() - trimmed.trim_start().len();
        if trimmed.trim_start().starts_with("paths:") {
            reading = true;
            // `paths: ['a']` on one line, which is legal but unusual; take the bracket contents
            let rest = trimmed.trim_start().trim_start_matches("paths:").trim();
            if rest.starts_with('[') {
                for entry in rest.trim_matches(|c| c == '[' || c == ']').split(',') {
                    paths.push(
                        entry
                            .trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string(),
                    );
                }
                reading = false;
            }
            continue;
        }
        if reading {
            // the list ends at the first line that is not an item of it
            let item = trimmed.trim_start();
            if indent > 0 && (item.starts_with("- ") || item == "-") {
                paths.push(
                    item.trim_start_matches("- ")
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string(),
                );
            } else {
                reading = false;
            }
        }
    }
    if paths.is_empty() {
        None
    } else {
        Some(paths)
    }
}

/// Whether a workflow path filter watches the app: `apps/flutter/**`, the wider `**`, or a prefix above it such as `apps/**`.
fn covers_the_app(pattern: &str) -> bool {
    let pattern = pattern.trim();
    if pattern == "**" || pattern == "**/*" {
        return true;
    }
    let directory = pattern.trim_end_matches("/**").trim_end_matches('*');
    let directory = directory.trim_end_matches('/');
    "apps/flutter".starts_with(directory) && !directory.is_empty()
}

/// The tests that need a real Postgres, and whether the pipeline gives them one.
///
/// A test that skips is a test nobody runs, and the deployment-store suite is the one that proves the engine answers from the
/// store a deployment actually uses. Preflight starts a throwaway server so those tests run on a laptop; the pipeline has to
/// do the same, by starting a `postgres` service and naming its connection string in `VIMBAI_TEST_POSTGRES`. Without it the
/// job is green because the tests never ran, which reads exactly like green because they passed.
pub fn postgres_test_problems(root: &Path) -> Vec<String> {
    let readers = test_files_reading(root, "VIMBAI_TEST_POSTGRES");
    if readers.is_empty() {
        return vec![
            "no test reads VIMBAI_TEST_POSTGRES, so the scan that watches the pipeline has nothing to hold"
                .to_string(),
        ];
    }
    let pipeline =
        std::fs::read_to_string(root.join(".github/workflows").join(GATING_RUST_PIPELINE))
            .unwrap_or_default();
    if pipeline.is_empty() {
        return vec![format!(
            "{GATING_RUST_PIPELINE} is gone, so nothing runs the workspace suite in CI"
        )];
    }
    let mut problems = Vec::new();
    if !pipeline.contains("VIMBAI_TEST_POSTGRES") {
        problems.push(format!(
            "{GATING_RUST_PIPELINE} never sets VIMBAI_TEST_POSTGRES, so {} skip instead of testing the deployment store",
            readers.join(", ")
        ));
    }
    if !pipeline.contains("image: postgres") {
        problems.push(format!(
            "{GATING_RUST_PIPELINE} names VIMBAI_TEST_POSTGRES but starts no Postgres service for it to reach"
        ));
    }
    problems
}

/// Every `.rs` file under `rust/` that mentions `needle`, relative to the repository root.
fn test_files_reading(root: &Path, needle: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.join("rust")];
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
            // the declaration in the gate's own source is not a test that needs a server
            if text.contains(needle) && !path.ends_with("xtask/src/workflows.rs") {
                if let Ok(relative) = path.strip_prefix(root) {
                    found.push(relative.to_string_lossy().into_owned());
                }
            }
        }
    }
    found.sort();
    found
}

/// Steps that run a test suite and then swallow its verdict.
///
/// `go test ./... || echo "No tests found"` is the shape to look for: the command fails, the step succeeds, and the job is
/// green. A test suite nobody can fail is a comment. Only test commands count - a `|| true` after a scanner with no tests to
/// find is a different thing, and named as such when it matters.
pub fn swallowed_test_failures(root: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    for (name, text) in workflow_files(root) {
        for (number, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || !runs_a_test_suite(trimmed) {
                continue;
            }
            let swallows = trimmed.contains("|| true")
                || trimmed.contains("|| echo")
                || trimmed.contains("|| :")
                || trimmed.ends_with("|| exit 0");
            if swallows {
                problems.push(format!(
                    "{name}:{} runs a test suite and swallows its verdict, so a failing test passes",
                    number + 1
                ));
            }
        }
    }
    problems
}

/// Whether a line of a workflow actually runs a test suite.
fn runs_a_test_suite(line: &str) -> bool {
    [
        "go test",
        "cargo test",
        "flutter test",
        "dart test",
        "npm test",
    ]
    .iter()
    .any(|command| line.contains(command))
}

/// The pipeline index, kept honest.
///
/// `.github/workflows/README.md` is the one place a reader learns what gates a change, so it has to name every workflow that
/// exists and no workflow that does not. Both halves matter: an undocumented gate is one nobody knows is protecting them, and
/// a documented gate that was deleted is a promise that has stopped being kept.
pub fn documented_workflows(root: &Path) -> Vec<String> {
    let directory = root.join(".github/workflows");
    let index_path = directory.join("README.md");
    let Ok(index) = std::fs::read_to_string(&index_path) else {
        return vec!["the workflow index is gone, so nothing says what gates a change".to_string()];
    };
    let existing: Vec<String> = workflow_files(root)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    if existing.is_empty() {
        return vec![
            "there are no workflows at all, so the index has nothing to describe".to_string(),
        ];
    }
    let mut problems = Vec::new();
    for name in &existing {
        if !index.contains(name.as_str()) {
            problems.push(format!(
                "{name} is not named in .github/workflows/README.md"
            ));
        }
    }
    // the other direction: a `.yml` the index presents as current has to exist. A line that says it was removed is not
    // claiming it still runs, and saying plainly what was deleted and why is worth more than never naming it.
    for line in index.lines() {
        if names_a_removal(line) {
            continue;
        }
        for word in line.split(|character: char| {
            character.is_whitespace() || character == '`' || character == '|' || character == ','
        }) {
            // a sentence ends in a full stop, and `gone.yml.` names the same file as `gone.yml`
            let candidate = word.trim_matches(|character: char| {
                matches!(character, ')' | '(' | '.' | ':' | ';' | '"')
            });
            if candidate.ends_with(".yml") && !existing.iter().any(|name| name == candidate) {
                problems.push(format!(
                    "the workflow index names {candidate}, which does not exist"
                ));
            }
        }
    }
    problems
}

/// Whether a line of the index is describing something that was removed rather than something that runs.
fn names_a_removal(line: &str) -> bool {
    let lowered = line.to_lowercase();
    // not the bare word "gone": a file called `gone.yml` would silence the check about itself
    [
        "removed",
        "deleted",
        "no longer",
        "stopped running",
        " is gone",
        " was ",
        " were ",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

/// The workflow that gates a merge.
const GATING_PIPELINE: &str = "ci-main.yml";

/// The workflow that runs the Rust workspace suite.
const GATING_RUST_PIPELINE: &str = "library-ci.yml";

#[cfg(test)]
mod trigger_rule_tests {
    use super::{covers_the_app, trigger_paths};

    #[test]
    fn path_filters_are_read_from_the_trigger_block_only() {
        let text = "on:\n  push:\n    paths:\n      - 'rust/**'\n      - \"apps/flutter/**\"\njobs:\n  x:\n    steps:\n      - run: echo\n        paths: nothing here\n";
        assert_eq!(
            trigger_paths(text),
            Some(vec!["rust/**".to_string(), "apps/flutter/**".to_string()])
        );
        // no filter at all is the strong case, not an empty one
        assert_eq!(trigger_paths("on:\n  push:\n\njobs:\n"), None);
        // and the single-line form
        assert_eq!(
            trigger_paths(
                "on:\n  pull_request:\n    paths: ['apps/flutter/**', 'rust/**']\njobs:\n"
            ),
            Some(vec!["apps/flutter/**".to_string(), "rust/**".to_string()])
        );
    }

    #[test]
    fn a_filter_covers_the_app_only_from_above_it() {
        assert!(covers_the_app("apps/flutter/**"));
        assert!(covers_the_app("apps/**"));
        assert!(covers_the_app("**"));
        assert!(covers_the_app("apps/flutter"));
        // a sibling, a child, and a string that merely starts the same
        assert!(!covers_the_app("rust/**"));
        assert!(!covers_the_app("apps/flutter/lib/**"));
        assert!(!covers_the_app("apps/other/**"));
        assert!(!covers_the_app("**/*.go"));
    }
}
