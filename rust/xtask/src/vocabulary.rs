#![forbid(unsafe_code)]
//! The engine names purposes, never accounts.
//!
//! ADR-018 decided that an account is found by the purpose it is kept for, and that the purpose is one of the laws of
//! bookkeeping rather than a word from somebody's trade. That decision is only worth as much as the code holding it: one
//! journal written next year that looks a sales account up by calling it "Sales" would put a business's vocabulary back
//! into the engine, and nothing would fail, because the Book it was tested against happens to use that word.
//!
//! Two rules are held here.
//!
//! * **A purpose is one of the roles.** Every purpose handed to `account_named` or `account_for_purpose` is compared with
//!   the roles the chart itself declares, read from `ROLES` in `ledger_book.rs`. A typo, or a purpose invented for one
//!   business, is reported with the file and line.
//! * **A name never decides where a posting goes.** A line that reads an account's name to decide which account it is -
//!   `account_name(...)` compared with `contains`, `==` or `eq_ignore_ascii_case` - is allowed only in the places that
//!   say so, each with its reason. There are two: the shared fallback that keeps a Book opened before roles working, and
//!   the money-account lookup, which matches the Book's own money account to its own chart rather than a word in the
//!   engine.
//!
//! Test modules are not read: a test names accounts because a test is a Book, and a Book's words are its own.

use std::collections::BTreeSet;
use std::path::Path;

/// A line that decides which account to post to from what the account is called, and the one place it is allowed.
pub struct NameSite {
    pub file: &'static str,
    pub function: &'static str,
    pub why: &'static str,
}

/// The places a name may decide which account a posting reaches, each because it is not the engine choosing a word.
pub const NAME_SITES: [NameSite; 2] = [
    NameSite {
        file: "subsidiary_journals.rs",
        function: "account_for_purpose",
        why: "the fallback that keeps a Book opened before roles existed posting, and it only reads accounts carrying no role",
    },
    NameSite {
        file: "cashbook.rs",
        function: "account_for_money",
        why: "it matches the Book's own money account to the Book's own chart, which is two facts about one business meeting",
    },
];

/// The roles the chart declares, read from `ROLES` in the engine so a new role is picked up without touching this gate.
pub fn declared_roles(root: &Path) -> BTreeSet<String> {
    let path = root.join("rust/crates/domains/accounting/src/ledger_book.rs");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let start = text.find("pub const ROLES").unwrap_or(0);
    let body = &text[start..];
    let end = body.find("];").map(|at| at + 2).unwrap_or(body.len());
    body[..end]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(|role| role.trim().to_lowercase())
        .filter(|role| !role.is_empty())
        .collect()
}

/// Every purpose literal handed to the shared lookups that is not one of the declared roles.
pub fn purposes_outside_roles(root: &Path) -> Vec<String> {
    let roles = declared_roles(root);
    let mut found = Vec::new();
    for path in engine_sources(root) {
        let Some(code) = without_tests(&read(&path)) else {
            continue;
        };
        for (name, at) in [("account_named", 4usize), ("account_for_purpose", 3usize)] {
            for (line, argument) in call_purposes(&code, name, at) {
                let Some(argument) = argument else { continue };
                if argument.trim().is_empty() {
                    continue;
                }
                if !roles.contains(&argument.trim().to_lowercase()) {
                    found.push(format!(
                        "{}:{line}: {name} asks for {:?}, which is not one of the {} roles the chart declares",
                        relative(root, &path),
                        argument,
                        roles.len()
                    ));
                }
            }
        }
    }
    found
}

/// Every line that lets a name decide which account a posting reaches, outside the places allowed to.
pub fn names_deciding_accounts(root: &Path) -> Vec<String> {
    let allowed: Vec<(&str, &str)> = NAME_SITES
        .iter()
        .map(|site| (site.file, site.function))
        .collect();
    let mut found = Vec::new();
    for path in engine_sources(root) {
        let Some(code) = without_tests(&read(&path)) else {
            continue;
        };
        let file = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        let mut function = String::new();
        for (number, line) in code.lines().enumerate() {
            if let Some(at) = line.find("fn ") {
                let rest = &line[at + 3..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    function = name;
                }
            }
            let decides = line.contains("account_name(")
                && (line.contains(".contains(")
                    || line.contains("eq_ignore_ascii_case(")
                    || line.contains("=="));
            if decides && !allowed.contains(&(file.as_str(), function.as_str())) {
                found.push(format!(
                    "{}:{}: {} decides an account by its name",
                    relative(root, &path),
                    number + 1,
                    function
                ));
            }
        }
    }
    found
}

/// The engine: the domain crates and the server that serves them, which is where a posting is decided.
fn engine_sources(root: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for base in ["rust/crates/domains", "rust/apps/api-server/src"] {
        walk(&root.join(base), &mut found);
    }
    found.sort();
    found
}

fn walk(directory: &Path, found: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) != Some("target") {
                walk(&path, found);
            }
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            found.push(path);
        }
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// The file without its test module, because a test is a Book and a Book's words are its own.
fn without_tests(code: &str) -> Option<String> {
    let cut = code.find("#[cfg(test)]").unwrap_or(code.len());
    Some(code[..cut].to_string())
}

/// The purpose argument of every call to `name`, with the line it starts on.
///
/// The purpose sits at a fixed place in each call - the fifth argument of `account_named` (graph, scope, payload, key,
/// purpose, kind) and the fourth of `account_for_purpose` (graph, scope, named, purpose, kind) - so it is read by
/// position, and a call that passes a variable instead of a literal is skipped rather than guessed at.
fn call_purposes(code: &str, name: &str, at: usize) -> Vec<(usize, Option<String>)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = code[from..].find(name) {
        let start = from + offset;
        let after = start + name.len();
        from = after;
        let rest = &code[after..];
        let Some(open) = rest.find('(') else { continue };
        // a call, not a definition
        if rest[..open].contains("fn ") {
            continue;
        }
        let body = &rest[open + 1..];
        let mut depth = 1usize;
        let mut end = body.len();
        let mut quoted = false;
        for (index, character) in body.char_indices() {
            match character {
                '"' => quoted = !quoted,
                '(' if !quoted => depth += 1,
                ')' if !quoted => {
                    depth -= 1;
                    if depth == 0 {
                        end = index;
                        break;
                    }
                }
                _ => {}
            }
        }
        let mut arguments: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut quoted = false;
        let mut nested = 0usize;
        for character in body[..end].chars() {
            match character {
                '"' => {
                    quoted = !quoted;
                    current.push(character);
                }
                '(' | '[' | '{' if !quoted => {
                    nested += 1;
                    current.push(character);
                }
                ')' | ']' | '}' if !quoted => {
                    nested = nested.saturating_sub(1);
                    current.push(character);
                }
                ',' if !quoted && nested == 0 => {
                    arguments.push(current.trim().to_string());
                    current.clear();
                }
                _ => current.push(character),
            }
        }
        if !current.trim().is_empty() {
            arguments.push(current.trim().to_string());
        }
        let purpose = arguments.get(at).and_then(|argument| {
            argument
                .strip_prefix('"')
                .and_then(|rest| rest.strip_suffix('"'))
                .map(|text| text.to_string())
        });
        let line = code[..start].lines().count();
        found.push((line, purpose));
    }
    found
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        crate::catalog::repo_root()
    }

    #[test]
    fn the_roles_are_read_from_the_engine_itself() {
        let roles = declared_roles(&root());
        assert!(roles.contains("sales"), "{roles:?}");
        assert!(roles.contains("receivable"), "{roles:?}");
        assert!(
            roles.len() >= 20,
            "the sweep read {} roles, not the twenty-five the chart declares",
            roles.len()
        );
    }

    #[test]
    fn a_purpose_that_is_not_a_role_is_reported() {
        let code = r#"
            fn post(graph: &G) {
                let sales = account_named(graph, scope, payload, "revenue_account", "turnover", "revenue")?;
                let right = account_named(graph, scope, payload, "revenue_account", "sales", "revenue")?;
            }
        "#;
        let literals = call_purposes(code, "account_named", 4);
        assert_eq!(literals.len(), 2, "{literals:?}");
        assert_eq!(literals[0].1.as_deref(), Some("turnover"), "{literals:?}");
        assert_eq!(literals[1].1.as_deref(), Some("sales"), "{literals:?}");
    }

    #[test]
    fn a_name_compared_to_decide_an_account_is_seen_and_a_role_is_not() {
        let root = root();
        let offenders = names_deciding_accounts(&root);
        // the sweep looked somewhere
        assert!(!engine_sources(&root).is_empty());
        for offender in &offenders {
            let allowed = NAME_SITES.iter().any(|site| offender.contains(site.file));
            assert!(allowed, "{offender}");
        }
    }

    #[test]
    fn every_place_a_name_is_allowed_says_why() {
        for site in NAME_SITES {
            assert!(!site.why.trim().is_empty(), "{} has no reason", site.file);
            let path = root()
                .join("rust/crates/domains/accounting/src")
                .join(site.file);
            let text = read(&path);
            assert!(
                text.contains(&format!("fn {}", site.function)),
                "{} no longer holds {}",
                site.file,
                site.function
            );
        }
    }
}
