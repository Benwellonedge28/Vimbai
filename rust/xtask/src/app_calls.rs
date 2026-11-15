//! The prefixes the app calls, read out of its own source.
//!
//! The app builds every address from one place - `${AppConfig.apiUrl}/something` - and nothing checked that `something` was a
//! prefix the gateway routes. Two were not: the budget surfaces moved to `/budgeting` and the app still called `/budget`, and
//! `/ratio-analysis` was a prefix no entry has ever routed. A screen that calls an address nobody serves fails as a 404 that
//! reads like a bug in the screen.
//!
//! Reading the source rather than a list kept beside it is the point: the list would be one more thing to forget.

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

#[cfg(test)]
mod tests {
    use super::scan;

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
