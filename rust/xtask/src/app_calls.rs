//! The prefixes the app calls, read out of its own source.
//!
//! The app builds every address from one place - `${AppConfig.apiUrl}/something` - and nothing checked that `something` was a
//! prefix the gateway routes. Two were not: the budget surfaces moved to `/budgeting` and the app still called `/budget`, and
//! `/ratio-analysis` was a prefix no entry has ever routed. A screen that calls an address nobody serves fails as a 404 that
//! reads like a bug in the screen.
//!
//! Reading the source rather than a list kept beside it is the point: the list would be one more thing to forget.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

use walk::dart_files;

/// Every top-level prefix the app names, without its leading slash, sorted and deduplicated.
pub fn prefixes(root: &Path) -> Vec<String> {
    let lib = root.join("apps/flutter/lib");
    let mut found: Vec<String> = Vec::new();
    for file in dart_files(&lib) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for prefix in scan(&text) {
            found.push(prefix);
        }
    }
    found.sort();
    found.dedup();
    found
}

/// The prefixes in one file's text.
fn scan(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    // `${AppConfig.apiUrl}/budgeting` - the one way the app spells an address
    let needle = "AppConfig.apiUrl}/";
    let mut from = 0;
    while let Some(offset) = text[from..].find(needle) {
        let start = from + offset + needle.len();
        let rest = &text[start..];
        let end = rest
            .find(|character: char| {
                !(character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
            })
            .unwrap_or(rest.len());
        // a second segment here would be the address itself (`/identity/users`) rather than the prefix it lives under, so the
        // prefix is the first segment and the rest is what the caller asks it for
        if end > 0 {
            found.push(rest[..end].to_string());
        }
        from = start + end.max(1);
    }
    found
}

/// A file-and-line reader for the app's Dart sources, so the scan has one place to walk the tree.
mod walk {
    use std::path::{Path, PathBuf};

    /// Every `.dart` file under a directory, sorted so two runs agree.
    pub fn dart_files(directory: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        collect(directory, &mut files);
        files.sort();
        files
    }

    fn collect(directory: &Path, files: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        let mut children: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        children.sort();
        for child in children {
            if child.is_dir() {
                collect(&child, files);
            } else if child.extension().and_then(|extension| extension.to_str()) == Some("dart") {
                files.push(child);
            }
        }
    }
}

/// One address the app builds, with the file and line it is written on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Call {
    pub file: String,
    pub line: usize,
    pub path: String,
}

/// Every address the app builds, resolved through the constants it builds them from.
///
/// The prefix scan above answers "does a door exist for this prefix". It cannot answer "does anything answer *this*
/// address", and the difference is not academic: `/accounting/accounts/` is under a door that exists, and the engine answers
/// `/accounting/accounts`, so the chart of accounts page failed on a fresh Book while every check passed.
///
/// One shape this scan cannot follow: a literal that begins `/` and is handed to a helper which prepends a base
/// (`_u('/books/$id/entries')`). Those addresses live under the `/book-sync` door, which the gateway forwards into the
/// engine's own `/books`, and the offline runtime tests are what prove that surface.
///
/// The app does not write an address in one place, so the scan follows what it reads: `AppConfig.apiUrl` is the empty base,
/// `AppConfig.accountingRoute` is `apiUrl/accounting`, and a file's own `_baseUrl` is whichever of those it was set to. An
/// interpolated segment (`$bookId`, `${id}`) is a wildcard, because the scan cannot know which Book a person opens.
pub fn calls(root: &Path) -> Vec<Call> {
    let lib = root.join("apps/flutter/lib");
    let config = std::fs::read_to_string(lib.join("app/config.dart")).unwrap_or_default();
    let mut routes = constants(&config);
    // the gateway base itself: everything the app calls is built from it, so it resolves to the empty path
    routes.insert("apiUrl".to_string(), String::new());
    let mut found: Vec<Call> = Vec::new();
    for file in dart_files(&lib) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let mut bases = routes.clone();
        bases.extend(constants(&text));
        for (line, literal) in literals(&text) {
            if declares_a_constant(&text, line) {
                continue;
            }
            if let Some(path) = resolve(&literal, &bases) {
                found.push(Call {
                    file: file
                        .strip_prefix(&lib)
                        .unwrap_or(&file)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    line,
                    path,
                });
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Whether this line declares a constant rather than calls something. A base is not an address: `AppConfig.accountingRoute`
/// is what the addresses are built from, and asking whether the engine answers `/accounting` is asking the wrong question.
fn declares_a_constant(text: &str, line: usize) -> bool {
    let Some(source) = text.lines().nth(line.saturating_sub(1)) else {
        return false;
    };
    let trimmed = source.trim();
    let declaration = trimmed
        .strip_prefix("static ")
        .or_else(|| trimmed.strip_prefix("final "))
        .or_else(|| trimmed.strip_prefix("const "));
    declaration.is_some_and(|rest| rest.contains(" = "))
}

/// The `name = 'value'` constants a Dart file declares, so a literal can be followed back to what it is a part of.
///
/// A constant written against `AppConfig.<something>` keeps that reference, so the walk upwards happens in [resolve] where a
/// missing name means "not an address" rather than a wrong one.
fn constants(text: &str) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for (name, value) in environment_defaults(text) {
        found.insert(name, value);
    }
    let lines: Vec<&str> = text.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        // `name = value` on one line, or `name =` with the value on the line below, which is how a long base is written
        let (declaration, value) = match trimmed.split_once(" = ") {
            Some((declaration, value)) => (declaration, without_comment(value)),
            None => match trimmed.strip_suffix('=') {
                Some(declaration) => (declaration.trim_end(), ""),
                None => continue,
            },
        };
        let Some(name) = declaration.split_whitespace().last() else {
            continue;
        };
        if !declaration.starts_with("final ") && !declaration.contains(" const ") {
            continue;
        }
        let name = name.trim_end_matches(';');
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        // the value may sit on the line below: `final String _baseUrl =` then `AppConfig.accountingRoute;`
        let value = if value.is_empty() {
            lines
                .iter()
                .skip(index + 1)
                .map(|next| without_comment(next))
                .find(|next| !next.is_empty())
                .unwrap_or("")
        } else {
            value
        };
        let value = value.trim_end_matches(';').trim();
        if let Some(inner) = quoted(value) {
            found.insert(name.to_string(), inner);
        } else if let Some(reference) = value.strip_prefix("AppConfig.") {
            found.insert(name.to_string(), format!("${{AppConfig.{reference}}}"));
        }
    }
    found
}

/// A line without its trailing comment, because a comment after a value is not part of the value.
fn without_comment(line: &str) -> &str {
    let trimmed = line.trim();
    match trimmed.find("//") {
        Some(at) => trimmed[..at].trim_end(),
        None => trimmed,
    }
}

/// Constants whose value is a `String.fromEnvironment` default, which is written across several lines.
fn environment_defaults(text: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("String.fromEnvironment(") {
        let before = &rest[..at];
        let name = before
            .rsplit(" = ")
            .next()
            .unwrap_or("")
            .trim()
            .trim_end_matches(';')
            .split_whitespace()
            .last()
            .unwrap_or("")
            .to_string();
        let after = &rest[at..];
        if let Some(default) = after.find("defaultValue:") {
            let value = &after[default + "defaultValue:".len()..];
            if let Some(inner) = quoted(value.trim_start()) {
                found.push((name, inner));
            }
        }
        rest = &after["String.fromEnvironment(".len()..];
    }
    found
}

/// The text inside a literal that is nothing but a literal.
fn quoted(value: &str) -> Option<String> {
    let trimmed = value.trim();
    for quote in ['\'', '"'] {
        if trimmed.len() >= 2 && trimmed.starts_with(quote) && trimmed.ends_with(quote) {
            return Some(trimmed[1..trimmed.len() - 1].to_string());
        }
    }
    None
}

/// Every string literal in a file, with the line it starts on.
fn literals(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let mut characters = line.chars().peekable();
        while let Some(character) = characters.next() {
            if character != '\'' && character != '"' {
                continue;
            }
            let quote = character;
            let mut literal = String::new();
            while let Some(next) = characters.next() {
                if next == '\\' {
                    if let Some(escaped) = characters.next() {
                        literal.push(escaped);
                    }
                    continue;
                }
                if next == quote {
                    break;
                }
                literal.push(next);
            }
            if !literal.is_empty() {
                found.push((index + 1, literal));
            }
        }
    }
    found
}

/// The path a literal names, when it starts from a base this scan knows.
fn resolve(literal: &str, bases: &BTreeMap<String, String>) -> Option<String> {
    let mut rest = expand_once(literal, bases)?;
    // a base may itself be built from another base (`_baseUrl` from `AppConfig.accountingRoute` from `$apiUrl`)
    for _ in 0..6 {
        match expand_once(&rest, bases) {
            Some(next) if next != rest => rest = next,
            _ => break,
        }
    }
    if !rest.starts_with('/') {
        return None;
    }
    let path = rest.split('?').next().unwrap_or("").to_string();
    let path = wildcards(&path);
    if !is_path_shape(&path) {
        return None;
    }
    Some(path)
}

/// Substitute a leading `$name` or `${name}` with what this scan knows it to be.
fn expand_once(literal: &str, bases: &BTreeMap<String, String>) -> Option<String> {
    let rest = literal;
    if !rest.starts_with('$') {
        return None;
    }
    if let Some(inner) = rest.strip_prefix("${") {
        let end = inner.find('}')?;
        let name = inner[..end]
            .trim()
            .trim_start_matches("AppConfig.")
            .to_string();
        let value = bases.get(&name)?;
        return Some(format!("{value}{}", &inner[end + 1..]).replace("AppConfig.", ""));
    }
    let name: String = rest
        .trim_start_matches('$')
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
        .collect();
    let name = name.trim_start_matches("AppConfig.").to_string();
    if name.is_empty() {
        return None;
    }
    let value = bases.get(&name)?;
    Some(format!("{value}{}", &rest[1 + name.len()..]).replace("AppConfig.", ""))
}

/// Every `$name` and `${...}` left in a path as the wildcard the engine records a parameter as.
fn wildcards(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut characters = path.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '$' {
            out.push(character);
            continue;
        }
        if characters.peek() == Some(&'{') {
            for next in characters.by_ref() {
                if next == '}' {
                    break;
                }
            }
        } else {
            while characters
                .peek()
                .is_some_and(|next| next.is_alphanumeric() || *next == '_')
            {
                characters.next();
            }
        }
        out.push_str("{}");
    }
    out
}

/// A path with a leading slash whose literal segments are plain and whose variables stand alone as `{}`.
fn is_path_shape(path: &str) -> bool {
    path.starts_with('/')
        && path.len() > 1
        && !path.contains(' ')
        && !path.contains('\'')
        && !path.contains('"')
}

/// The addresses the app calls that nothing answers: the prefix exists, the address does not.
pub fn unanswered(root: &Path) -> Vec<Call> {
    let answered: Vec<String> = crate::gateway::engine_routes(root).into_iter().collect();
    let routed: BTreeSet<String> = crate::catalog::rows(root)
        .into_iter()
        .map(|row| row.gateway_path.trim_end_matches('/').to_string())
        .collect();
    // a retired service's prefix the engine answers behind: `/multimodal/tasks` at the door is `/tasks` in the engine.
    // Without these, every address a phone reads through one of those doors looked unanswered, so a wrong one hid among
    // them - which is how `/multimodal/tasks/` sat in the app unread.
    let doors = crate::gateway::doors(root);
    calls(root)
        .into_iter()
        .filter(|call| {
            // the path as written: `gateway::shape` trims a trailing slash, and the trailing slash is the defect being looked
            // for, so shaping here keeps it
            let path = call.path.clone();
            // a door: the gateway answers it and forwards what is behind it. `/identity` signs people in, `/book-sync`
            // forwards into the engine's `/books`, and a routed prefix is itself a door.
            if DOORS.iter().any(|door| path == *door) || routed.contains(&path) {
                return false;
            }
            if answered.iter().any(|engine| matches(engine, &path)) {
                return false;
            }
            let through_a_door = doors
                .iter()
                .filter_map(|door| crate::gateway::through(door, &path));
            // the address is compared as written: shaping it would trim the trailing slash, which is the defect being looked for
            !through_a_door
                .into_iter()
                .any(|address| answered.iter().any(|engine| matches(engine, &address)))
        })
        .collect()
}

/// Doors the gateway answers itself, before any service is asked.
const DOORS: [&str; 4] = ["/identity", "/book-sync", "/records", "/health"];

/// Whether an engine route answers a path, where the engine's `{}` stands for any one segment.
fn matches(engine: &str, path: &str) -> bool {
    let engine: Vec<&str> = engine.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    engine.len() == path.len()
        && engine
            .iter()
            .zip(path.iter())
            // an empty segment is a trailing slash, which is a different address from the one a parameter route answers
            .all(|(route, asked)| route == asked || (*route == "{}" && !asked.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::{constants, scan};

    #[test]
    fn a_base_written_across_two_lines_is_still_a_base() {
        // the shape that hid the capture service: the value sits under the name, and a comment follows it
        let text =
            "  final String _baseUrl =\n      AppConfig.multimodalRoute; // Via API Gateway\n";
        let bases = constants(text);
        assert_eq!(
            bases.get("_baseUrl").map(String::as_str),
            Some("${AppConfig.multimodalRoute}"),
            "a base declared on the line below its name has to resolve, or every address built from it is invisible"
        );
    }

    #[test]
    fn a_trailing_slash_survives_a_door() {
        let door = crate::gateway::Door {
            prefix: "/multimodal".to_string(),
            keeps_prefix: false,
        };
        assert_eq!(
            crate::gateway::through(&door, "/multimodal/tasks/").as_deref(),
            Some("/tasks/"),
            "the slash is the defect being looked for, so walking a door keeps it"
        );
        assert_eq!(
            crate::gateway::through(&door, "/multimodal/tasks").as_deref(),
            Some("/tasks")
        );
        assert_eq!(crate::gateway::through(&door, "/multimodalx/tasks"), None);
    }

    #[test]
    fn a_prefix_is_read_out_of_the_address_the_app_builds() {
        let text = "Uri.parse('${AppConfig.apiUrl}/identity/users/login')";
        assert_eq!(scan(text), vec!["identity".to_string()]);
    }

    #[test]
    fn a_dart_expression_after_the_slash_is_not_a_prefix() {
        let text = "final url = '${AppConfig.apiUrl}/${service.path}';";
        assert!(scan(text).is_empty(), "a variable is not a prefix");
    }
}

#[cfg(test)]
mod address_tests {
    use super::{matches, wildcards};

    #[test]
    fn a_parameter_answers_any_one_segment() {
        assert!(matches(
            "/accounting/accounts/{}",
            "/accounting/accounts/1000"
        ));
        assert!(matches("/calc/{}", "/calc/ratio-analysis-ratios"));
    }

    #[test]
    fn a_trailing_slash_is_not_a_parameter() {
        assert!(!matches("/accounting/accounts/{}", "/accounting/accounts/"));
        assert!(!matches("/accounting/accounts", "/accounting/accounts/"));
    }

    #[test]
    fn an_interpolation_becomes_one_wildcard_segment() {
        assert_eq!(wildcards("/books/$bookId/entries"), "/books/{}/entries");
        assert_eq!(wildcards("/books/${book.id}/entries"), "/books/{}/entries");
    }
}
