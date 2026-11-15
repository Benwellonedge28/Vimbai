#![forbid(unsafe_code)]

//! What the pipelines check, against what `scripts/preflight.sh` checks.
//!
//! The standing rule is to run preflight before every push, and that rule is only worth anything if a clean preflight means a
//! green pipeline. It did not: `ci-main.yml` has run `dart format --set-exit-if-changed` and `flutter analyze` since the app
//! job was written, and preflight ran `flutter test` alone - so an unformatted or unanalysed change passed preflight and failed
//! the pipeline. Nothing said so, because nothing compared the two.

use std::path::Path;

/// The subcommands that reach a verdict. `flutter pub get` and `go mod download` prepare a toolchain rather than judge it.
const VERDICTS: [&str; 6] = ["test", "clippy", "vet", "analyze", "fmt", "format"];

/// The toolchains preflight claims to cover, so a new check on any of them has to be added to preflight too.
const TOOLS: [&str; 4] = ["cargo", "go", "dart", "flutter"];

/// Every `tool subcommand` a check runs, as `cargo fmt`, with the workflow that runs it.
pub fn checks_in_workflows(root: &Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root.join(".github/workflows")) else {
        return found;
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".yml") || name.ends_with(".yaml"))
        .collect();
    names.sort();
    for name in names {
        let Ok(text) = std::fs::read_to_string(root.join(".github/workflows").join(&name)) else {
            continue;
        };
        for (tool, subcommand) in verdicts_in(&text) {
            found.push((format!("{tool} {subcommand}"), name.clone()));
        }
    }
    found
}

/// The checks `scripts/preflight.sh` runs, in the same form.
pub fn checks_in_preflight(root: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(root.join("scripts/preflight.sh")) else {
        return Vec::new();
    };
    verdicts_in(&text)
        .into_iter()
        .map(|(tool, subcommand)| format!("{tool} {subcommand}"))
        .collect()
}

/// A check the pipelines run and preflight does not, so a clean preflight does not mean a green pipeline.
pub fn checks_missing_from_preflight(root: &Path) -> Vec<String> {
    let preflight = checks_in_preflight(root);
    let workflows = checks_in_workflows(root);
    // A preflight that runs nothing is not clean just because there is nothing to compare with; there is always something.
    if preflight.is_empty() && workflows.is_empty() {
        return vec!["no check was found in the pipelines or in scripts/preflight.sh".to_string()];
    }
    let mut problems = Vec::new();
    for (check, workflow) in workflows {
        if !preflight.contains(&check) {
            problems.push(format!(
                "{workflow} runs `{check}` and scripts/preflight.sh does not, so a clean preflight can still fail the pipeline"
            ));
        }
    }
    problems
}

/// `tool subcommand` pairs for every verdict-reaching command in a workflow or a script.
fn verdicts_in(text: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for line in text.lines() {
        let code = line.split('#').next().unwrap_or("");
        // `cd rust && cargo test` is two commands, and the check is the second one
        for segment in code.split("&&").flat_map(|part| part.split('|')) {
            let tokens: Vec<&str> = segment
                .split_whitespace()
                .map(|token| token.trim_matches(|character: char| "()'\";".contains(character)))
                .filter(|token| !token.is_empty())
                .collect();
            // a workflow wraps its command in `run:`, sometimes as a list item, sometimes after sudo
            let mut tokens = tokens.as_slice();
            while let Some(first) = tokens.first() {
                if matches!(*first, "run:" | "-" | "sudo" | "env") {
                    tokens = &tokens[1..];
                } else {
                    break;
                }
            }
            let Some(tool) = tokens.first().copied() else {
                continue;
            };
            if !TOOLS.contains(&tool) {
                continue;
            }
            let Some(subcommand) = tokens.get(1).copied() else {
                continue;
            };
            if subcommand.starts_with('-') || !VERDICTS.contains(&subcommand) {
                continue;
            }
            let pair = (tool.to_string(), subcommand.to_string());
            if !found.contains(&pair) {
                found.push(pair);
            }
        }
    }
    found
}
