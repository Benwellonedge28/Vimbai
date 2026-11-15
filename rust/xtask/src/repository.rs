#![forbid(unsafe_code)]

//! Every file in the repository, once, for the checks that have to walk it.
//!
//! Two scans used to walk the tree with their own copy of this, and a third would have made a third. The build trees are
//! skipped by name because a `target/` full of generated bindings is not the repository, and the list is sorted because a scan
//! that reports in directory order reports differently on a different machine.

use std::path::Path;

/// Build output and editor state, never the repository's own files.
// `ephemeral` is Flutter's own build output: it writes a Python helper into ios/Flutter/ephemeral on every build, and
// a generated helper is not the repository deciding to keep Python.
const NOT_THE_REPOSITORY: [&str; 6] = [
    "target",
    ".git",
    "node_modules",
    ".dart_tool",
    "build",
    "ephemeral",
];

/// Every file under `root`, as absolute paths, sorted.
pub fn files(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    walk(root, &mut found);
    found.sort();
    found
}

fn walk(directory: &Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        let skipped = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| NOT_THE_REPOSITORY.contains(&name));
        if skipped {
            continue;
        }
        if path.is_dir() {
            walk(&path, found);
        } else if let Some(name) = path.to_str() {
            found.push(name.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_walk_finds_files_and_skips_the_build_trees() {
        let mut directory = std::env::temp_dir();
        directory.push(format!("vimbai-repository-walk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("rust/src")).expect("a source directory");
        std::fs::create_dir_all(directory.join("rust/target/debug")).expect("a build directory");
        std::fs::write(directory.join("rust/src/lib.rs"), "// lib\n").expect("a source file");
        std::fs::write(
            directory.join("rust/target/debug/generated.rs"),
            "// generated\n",
        )
        .expect("a generated file");

        let found = files(&directory);
        assert!(found.iter().any(|file| file.ends_with("rust/src/lib.rs")));
        assert!(
            !found.iter().any(|file| file.contains("/target/")),
            "a generated file was reported as one of the repository's"
        );

        std::fs::remove_dir_all(&directory).expect("the scratch directory goes");
    }
}
