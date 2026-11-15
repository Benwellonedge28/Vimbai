//! The Makefile, held to its own promises.
//!
//! A thin wrapper is easy to keep honest only while somebody reads it. This one had already drifted: `flutter-test` was never
//! declared `.PHONY`, so a directory of that name would have made `make flutter-test` decide there was nothing to do, and
//! `make help` - the only index of the file - described eight targets while the repository's gates had grown to eleven
//! commands. A person reading `make help` would have concluded the gates they could not see did not exist.
//!
//! Four things are checked, all mechanically:
//!
//! 1. **Every target is `.PHONY`.** A target that shares its name with a path can silently stop running.
//! 2. **`make help` describes every target**, so the file's own index is the file.
//! 3. **`make help` describes nothing that is not a target**, so the index has no dead entries.
//! 4. **Every `xtask` command the Makefile runs exists**, read from the binary's own dispatch.
#![forbid(unsafe_code)]

use std::path::Path;

/// Every problem the Makefile has with its targets, its help and the commands it calls.
pub fn makefile_claims(root: &Path) -> Vec<String> {
    let Ok(makefile) = std::fs::read_to_string(root.join("Makefile")) else {
        return vec!["there is no Makefile, so there is nothing to check".to_string()];
    };
    let targets = targets(&makefile);
    if targets.is_empty() {
        return vec![
            "the Makefile declares no targets, so the scan found nothing to check".to_string(),
        ];
    }
    let mut problems = Vec::new();

    let phony = phony_targets(&makefile);
    for target in &targets {
        if !phony.contains(target) {
            problems.push(format!("Makefile target `{target}` is not declared .PHONY"));
        }
    }

    let described = described_targets(&makefile);
    for target in &targets {
        if !described.contains(target) {
            problems.push(format!(
                "`make help` does not describe the target `{target}`"
            ));
        }
    }
    for name in &described {
        if !targets.contains(name) {
            problems.push(format!(
                "`make help` describes `{name}`, which is not a target"
            ));
        }
    }

    let implemented = crate::documentation::implemented_commands(root);
    if implemented.is_empty() {
        problems.push(
            "xtask implements no commands, so a command the Makefile runs cannot be checked"
                .to_string(),
        );
    } else {
        for command in xtask_commands(&makefile) {
            if !implemented.contains(&command) {
                problems.push(format!(
                    "the Makefile runs `xtask -- {command}`, which is not a command xtask has"
                ));
            }
        }
    }
    problems
}

/// Every target the Makefile declares, in declaration order.
pub fn targets(makefile: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for line in makefile.lines() {
        if line.starts_with([' ', '\t', '#']) {
            continue;
        }
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || name.starts_with('.') || name.contains('=') {
            continue;
        }
        // `:=`, `::` and a target with prerequisites are all fine; `%` marks a pattern rule, which is not a target a person runs
        if name.contains('%') || name.contains(' ') || !rest.is_empty() && rest.starts_with('=') {
            continue;
        }
        if !name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        }) {
            continue;
        }
        if !found.contains(&name.to_string()) {
            found.push(name.to_string());
        }
    }
    found
}

/// The targets declared `.PHONY`.
pub fn phony_targets(makefile: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in makefile.lines() {
        if let Some(rest) = line.trim_start().strip_prefix(".PHONY:") {
            for name in rest.split_whitespace() {
                if !found.contains(&name.to_string()) {
                    found.push(name.to_string());
                }
            }
        }
    }
    found
}

/// The targets `make help` prints a line about.
pub fn described_targets(makefile: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut inside_help = false;
    for line in makefile.lines() {
        if !line.starts_with([' ', '\t']) {
            inside_help = line.starts_with("help:");
            continue;
        }
        if !inside_help {
            continue;
        }
        let Some(at) = line.find("make ") else {
            continue;
        };
        let name: String = line[at + "make ".len()..]
            .chars()
            .take_while(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
            .collect();
        if !name.is_empty() && !found.contains(&name) {
            found.push(name);
        }
    }
    found
}

/// The `xtask` commands the Makefile runs, whether through its own variable or spelled out.
pub fn xtask_commands(makefile: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for line in makefile.lines() {
        for marker in ["$(XTASK)", "xtask --"] {
            let mut rest = line;
            while let Some(at) = rest.find(marker) {
                // `xtask --manifest-path` is a flag of cargo, not a command: only a marker followed by space introduces one
                let immediately = &rest[at + marker.len()..];
                if marker == "xtask --" && !immediately.starts_with(' ') && !immediately.is_empty()
                {
                    rest = immediately;
                    continue;
                }
                let after = immediately.trim_start();
                let command: String = after
                    .chars()
                    .take_while(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                    })
                    .collect();
                let command = command.trim_matches('-').to_string();
                if !command.is_empty() && !command.starts_with('$') && !found.contains(&command) {
                    found.push(command);
                }
                rest = &rest[at + marker.len()..];
            }
        }
    }
    found
}
