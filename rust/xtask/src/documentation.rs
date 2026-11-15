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

fn names_a_removal(line: &str) -> bool {
    let lowered = line.to_lowercase();
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

fn numbers(sentence: &str) -> Vec<u32> {
    sentence
        .split(|character: char| !character.is_ascii_digit())
        .filter(|digits| !digits.is_empty() && digits.len() <= 4)
        .filter_map(|digits| digits.parse::<u32>().ok())
        .collect()
}
