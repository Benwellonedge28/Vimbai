#![forbid(unsafe_code)]

//! The rules the repository keeps everywhere, and until now nothing was checking.
//!
//! Two standing requirements - safe Rust only, and no Python - were enforced by the pytest suite that has since been deleted.
//! Both are true today: every crate root carries the attribute and no `.py` file is tracked. That is exactly the state a rule
//! quietly stops being true in, because the day someone adds a test crate without the attribute, everything still builds.

use std::path::Path;

const ATTRIBUTE: &str = "#![forbid(unsafe_code)]";

/// The files that begin a crate: its library, its binary, its build script, and every integration target, because an
/// integration test is a crate of its own and its own `unsafe` would be nobody's lint.
pub fn crate_roots(root: &Path) -> Vec<String> {
    crate::repository::files(root)
        .into_iter()
        .filter(|file| is_a_crate_root(file))
        .collect()
}

fn is_a_crate_root(file: &str) -> bool {
    if !file.ends_with(".rs") {
        return false;
    }
    if file.ends_with("/src/lib.rs")
        || file.ends_with("/src/main.rs")
        || file.ends_with("/build.rs")
    {
        return true;
    }
    // the file's own folder decides, not the first folder of that name in the path: the integrity suite lives at
    // `rust/tests/financial-integrity/tests/invariants.rs`, and the outer `tests/` is just where the crate sits.
    let mut parts = file.rsplit('/');
    let file_name = parts.next().unwrap_or_default();
    let folder = parts.next().unwrap_or_default();
    !file_name.is_empty() && matches!(folder, "tests" | "benches" | "examples" | "bin")
}

/// A crate root without the attribute, or a repository with no crate roots to check.
pub fn roots_without_the_attribute(root: &Path) -> Vec<String> {
    let roots = crate_roots(root);
    if roots.is_empty() {
        return vec!["there are no crate roots to check for the safety attribute".to_string()];
    }
    roots
        .into_iter()
        .filter(|file| {
            std::fs::read_to_string(file)
                .map(|text| !text.contains(ATTRIBUTE))
                .unwrap_or(true)
        })
        .map(|file| format!("{file} does not begin with {ATTRIBUTE}"))
        .collect()
}

/// The `unsafe` keyword where it would actually do something: the attribute names it, and a comment may discuss it.
pub fn unsafe_that_does_something(root: &Path) -> Vec<String> {
    let mut findings = Vec::new();
    let mut scanned = 0usize;
    for file in crate::repository::files(root) {
        if !file.ends_with(".rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        scanned += 1;
        for (number, line) in text.lines().enumerate() {
            // the keyword is only code when what follows it can be governed: `unsafe {`, `unsafe fn`. Prose about unsafe - a
            // changelog line quoted in a string, a comment explaining why there is none - is the repository talking, not doing.
            // a gate states its rule inside a string literal, and a literal is not code: `!source.contains("unsafe {")` is the
            // assertion that the block is absent. The same stripper the crate scan uses answers it here.
            let code = crate::crates::strip_strings(line.split("//").next().unwrap_or(""));
            if governs_unsafe(&code) {
                findings.push(format!("{file}:{} is real unsafe code", number + 1));
            }
        }
    }
    if scanned == 0 {
        findings.push("there is no Rust source to scan for unsafe code".to_string());
    }
    findings
}

/// Whether a line of code writes the `unsafe` keyword in a position where it would do something.
fn governs_unsafe(code: &str) -> bool {
    let mut rest = code;
    while let Some(at) = rest.find("unsafe") {
        // `unsafe_code` and `unsafeish` are different words
        let before_is_a_boundary = at == 0
            || !rest[..at]
                .chars()
                .next_back()
                .is_some_and(|character| character.is_alphanumeric() || character == '_');
        let after = rest[at + "unsafe".len()..].trim_start();
        let after_is_a_boundary = after
            .chars()
            .next()
            .is_some_and(|character| !(character.is_alphanumeric() || character == '_'));
        if before_is_a_boundary && after_is_a_boundary {
            let governs = after.starts_with('{')
                || ["fn", "impl", "extern", "trait", "mod"]
                    .iter()
                    .any(|keyword| after.starts_with(keyword))
                || after.starts_with(')');
            if governs {
                return true;
            }
        }
        rest = &rest[at + "unsafe".len()..];
    }
    false
}

/// Any Python left in the repository, by extension or by the names a Python project keeps at its root.
pub fn python_files(root: &Path) -> Vec<String> {
    let files = crate::repository::files(root);
    if files.is_empty() {
        return vec!["the repository has no files to check for Python".to_string()];
    }
    files
        .into_iter()
        .filter(|file| {
            let name = file.rsplit('/').next().unwrap_or(file);
            file.ends_with(".py")
                || file.ends_with(".pyc")
                || file.ends_with(".pyi")
                || name == "pyproject.toml"
                || name == "setup.py"
                || name == "conftest.py"
                || name == "pytest.ini"
                || name.starts_with("requirements")
        })
        .map(|file| format!("{file} is Python"))
        .collect()
}
