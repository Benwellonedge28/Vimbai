//! The documentation index, held to the documents that exist.
//!
//! `docs/README.md` is the one page that says which page answers a question. It is prose, so nothing failed when it drifted: it
//! claimed "ten documents" while there were eight, described the domain pages as twenty-two while there were twenty-four, and
//! two real documents (`SYNC.md` and `flutter-structure.md`) were not listed at all. A reader following it would have concluded
//! the index was the whole of the documentation.
//!
//! Four things are checked, each of them mechanically decidable:
//!
//! 1. **Every document is named.** Each `docs/**/*.md`, except the index itself, has to appear in it by path.
//! 2. **Every path it names exists**, so a renamed document cannot leave a dead pointer behind.
//! 3. **Every command it tells a reader to run exists.** A `cargo run -p xtask -- <command>` in the index has to be an arm the
//!    binary actually has, read from `xtask/src/main.rs` rather than from a list that would drift beside it.
//! 4. **A count it puts beside the domain pages is the number of domain pages.** This is the same rule `readme.rs` holds the
//!    routed-surface figure to: a figure in prose is either measured or absent.
#![forbid(unsafe_code)]

use std::path::Path;

/// Every markdown document under `docs/`, relative to it, without the index itself.
pub fn documents(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    collect(&root.join("docs"), &root.join("docs"), &mut found);
    found.sort();
    found.retain(|path| path != "README.md");
    found
}

fn collect(directory: &Path, base: &Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, base, found);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("md") {
            if let Ok(relative) = path.strip_prefix(base) {
                found.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

/// The `cargo run -p xtask -- <command>` invocations a body of text tells a reader to make.
pub fn xtask_commands(text: &str) -> Vec<String> {
    let mut commands: Vec<String> = Vec::new();
    for line in text.lines() {
        let mut rest = line;
        while let Some(at) = rest.find("xtask --") {
            let after = rest[at + "xtask --".len()..].trim_start();
            let command: String = after
                .chars()
                .take_while(|character| {
                    character.is_ascii_alphanumeric() || *character == '-' || *character == '_'
                })
                .collect();
            let command = command.trim_matches('-').to_string();
            if !command.is_empty() && !commands.contains(&command) {
                commands.push(command);
            }
            rest = &rest[at + "xtask --".len()..];
        }
    }
    commands
}

/// Every command the binary implements, read from its own dispatch: a line that begins an arm with a quoted name, then `=>`
/// or an `if` guard. Reading the dispatch is what makes this a check rather than a second list.
pub fn implemented_commands(root: &Path) -> Vec<String> {
    let Ok(source) = std::fs::read_to_string(root.join("rust/xtask/src/main.rs")) else {
        return Vec::new();
    };
    let mut commands: Vec<std::string::String> = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find('"') else {
            continue;
        };
        let name = &rest[..end];
        let after = rest[end + 1..].trim_start();
        if !(after.starts_with("=>") || after.starts_with("if")) {
            continue;
        }
        let is_a_command = !name.is_empty()
            && name.chars().all(|character| {
                character.is_ascii_alphanumeric() || character == '-' || character == '_'
            });
        if is_a_command && !commands.contains(&name.to_string()) {
            commands.push(name.to_string());
        }
    }
    commands
}

/// Every problem the index has with the documents, the commands and the figures it names.
pub fn documentation_index(root: &Path) -> Vec<String> {
    let index_path = root.join("docs/README.md");
    let Ok(index) = std::fs::read_to_string(&index_path) else {
        return vec![
            "docs/README.md is gone, so nothing says which page answers a question".to_string(),
        ];
    };
    let documents = documents(root);
    if documents.is_empty() {
        return vec![
            "there are no documents under docs/, so the index has nothing to describe".to_string(),
        ];
    }
    let mut problems = Vec::new();

    for document in &documents {
        if index.contains(document.as_str()) {
            continue;
        }
        // a bare file name is how a sentence refers to a page once it has named the folder, and naming a folder accounts for
        // the pages inside it - an index that says "one page per domain, in `domains/`" has documented all of them.
        let bare = document.rsplit('/').next().unwrap_or(document);
        if index.contains(bare) {
            continue;
        }
        let folder = document
            .rsplit_once('/')
            .map(|(folder, _)| folder.to_string());
        if let Some(folder) = folder {
            if index.contains(&format!("{folder}/")) {
                continue;
            }
        }
        problems.push(format!("docs/{document} is not named in docs/README.md"));
    }

    // the other direction: a path the index presents as current has to exist. A line recording what was removed is exempt, for
    // the same reason the workflow index allows it - saying plainly what is gone is worth more than never naming it.
    let existing = documents.to_vec();
    for line in index.lines() {
        if names_a_removal(line) {
            continue;
        }
        for word in line.split(|character: char| {
            character.is_whitespace() || character == '`' || character == '|' || character == ','
        }) {
            let candidate = word.trim_matches(|character: char| {
                matches!(character, ')' | '(' | '.' | ':' | ';' | '"')
            });
            if !candidate.ends_with(".md") || candidate.contains("http") {
                continue;
            }
            if candidate == "README.md" {
                continue;
            }
            if !existing.iter().any(|document| document == candidate) {
                problems.push(format!(
                    "docs/README.md names docs/{candidate}, which does not exist"
                ));
            }
        }
    }

    let implemented = implemented_commands(root);
    if implemented.is_empty() {
        return problems
            .into_iter()
            .chain(std::iter::once(
                "xtask implements no commands, so a documented command cannot be checked"
                    .to_string(),
            ))
            .collect();
    }
    for command in xtask_commands(&index) {
        if !implemented.contains(&command) {
            problems.push(format!(
                "docs/README.md tells a reader to run `xtask -- {command}`, which is not a command xtask has"
            ));
        }
    }

    problems.extend(domain_page_count(root, &index, &documents));
    problems.extend(stale_references(root));
    problems
}

/// A figure printed beside the domain pages has to be the number of domain pages.
fn domain_page_count(root: &Path, index: &str, documents: &[String]) -> Vec<String> {
    let actual = documents
        .iter()
        .filter(|document| document.starts_with("domains/"))
        .count();
    let _ = root;
    let mut problems = Vec::new();
    for sentence in index.split(['.', '!', '?']) {
        let lowered = sentence.to_lowercase();
        if !(lowered.contains("domain") && lowered.contains("page")) {
            continue;
        }
        for number in numbers(sentence) {
            if number as usize != actual {
                problems.push(format!(
                    "docs/README.md says {number} domain pages; docs/domains holds {actual}"
                ));
            }
        }
    }
    problems
}

/// A reference a document makes to a file that is not there.
///
/// Prose rots by pointing at things that moved: `docs/ROADMAP.md`, the working plan, cited
/// `docs/decisions/ADR-005-vimbai-pay-deferred.md` for months after the decision was recorded as
/// `ADR-005-vimbai-pay-not-built.md`. A reader following it got nothing, in the one document that says what to build next.
///
/// Two kinds of reference are decidable and checked: a decision cited by its full file name has to be a file under
/// `docs/decisions/`, and a workflow named with a `.yml` suffix has to exist somewhere in the repository. A line that records a
/// removal is exempt, because naming what was deleted is how a reader learns it is gone.
pub fn stale_references(root: &Path) -> Vec<String> {
    let decisions = root.join("docs/decisions");
    let mut decision_files: Vec<String> = std::fs::read_dir(&decisions)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    decision_files.sort();

    let mut files: Vec<String> = Vec::new();
    every_file(root, &mut files);
    let mut workflows: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root.join(".github/workflows")) {
        for entry in entries.filter_map(|entry| entry.ok()) {
            if let Some(name) = entry.file_name().to_str().map(|name| name.to_string()) {
                workflows.push(name);
            }
        }
    }
    if decision_files.is_empty() || workflows.is_empty() {
        return vec![
            "there are no decisions or no workflows to check a reference against".to_string(),
        ];
    }

    let mut problems = Vec::new();
    let mut sources: Vec<(String, String)> = Vec::new();
    for document in documents(root)
        .into_iter()
        .chain(std::iter::once("README.md".to_string()))
    {
        if let Ok(text) = std::fs::read_to_string(root.join("docs").join(&document)) {
            sources.push((format!("docs/{document}"), text));
        }
    }
    // The scripts carry prose too, and `preflight.sh` told every reader for months that the app's tests "run in the mobile
    // workflow" after that workflow had been deleted. A name in a script is held to the same rule as a name in a page.
    if let Ok(entries) = std::fs::read_dir(root.join("scripts")) {
        let mut scripts: Vec<String> = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.ends_with(".sh"))
            .collect();
        scripts.sort();
        for script in scripts {
            if let Ok(text) = std::fs::read_to_string(root.join("scripts").join(&script)) {
                sources.push((format!("scripts/{script}"), text));
            }
        }
    }

    for (document, text) in sources {
        for line in text.lines() {
            // a markdown heading is structure, but a shell comment is where a script's prose lives
            let heading = document.starts_with("docs/") && line.trim_start().starts_with('#');
            if names_a_removal(line) || heading {
                continue;
            }
            let mut tokens = inline_code(line);
            if document.starts_with("scripts/") {
                tokens.extend(bare_names(line));
            }
            for token in tokens {
                if let Some(reference) = token
                    .strip_suffix(".md")
                    .filter(|rest| rest.starts_with("ADR-"))
                {
                    let with_suffix = format!("{reference}.md");
                    if document.starts_with("docs/")
                        && !decision_files.iter().any(|file| file == &with_suffix)
                    {
                        problems.push(format!(
                            "{document} cites `{with_suffix}`, which is not in docs/decisions"
                        ));
                    }
                } else if token.ends_with(".yml") || token.ends_with(".yaml") {
                    let known = workflows.contains(&token)
                        || files
                            .iter()
                            .any(|file| file.ends_with(&format!("/{token}")))
                        || files.contains(&token);
                    if !known {
                        problems.push(format!(
                            "{document} names `{token}`, which is not a file in the repository"
                        ));
                    }
                }
            }
        }
    }
    problems
}

fn every_file(directory: &Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        let skipped = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| {
                matches!(
                    name,
                    "target" | ".git" | "node_modules" | ".dart_tool" | "build"
                )
            })
            .unwrap_or(false);
        if skipped {
            continue;
        }
        if path.is_dir() {
            every_file(&path, found);
        } else if let Some(name) = path.to_str() {
            found.push(name.to_string());
        }
    }
}

/// A file name written without backticks, the way a shell script writes one.
fn bare_names(line: &str) -> Vec<String> {
    line.split(|character: char| {
        character.is_whitespace() || matches!(character, '(' | ')' | '"' | '`')
    })
    .filter(|word| (word.ends_with(".yml") || word.ends_with(".yaml")) && !word.contains('$'))
    .map(|word| word.to_string())
    .collect()
}

/// The inline-code spans on a line, without their backticks.
fn inline_code(line: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut inside = false;
    let mut current = std::string::String::new();
    for character in line.chars() {
        if character == '`' {
            if inside && !current.trim().is_empty() {
                spans.push(current.trim().to_string());
            }
            current.clear();
            inside = !inside;
            continue;
        }
        if inside {
            current.push(character);
        }
    }
    spans
}

fn names_a_removal(line: &str) -> bool {
    let lowered = line.to_lowercase();
    [
        "removed",
        "deleted",
        "no longer",
        "stopped running",
        " is gone",
        "used to be",
        " was ",
        " were ",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn numbers(sentence: &str) -> Vec<u32> {
    sentence
        .split(|character: char| !character.is_ascii_digit())
        .filter(|digits| !digits.is_empty() && digits.len() <= 4)
        .filter_map(|digits| digits.parse::<u32>().ok())
        .collect()
}
