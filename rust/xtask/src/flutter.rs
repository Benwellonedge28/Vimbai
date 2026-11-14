//! The Flutter app's structure, checked rather than trusted.
//!
//! P7 of the roadmap is a migration of 58 dart files out of five layer folders - `models`, `pages`, `services`, `widgets`,
//! `local_db` - into `lib/features/<feature>/`, plus `lib/core/` for what every feature shares and `lib/app/` for the
//! shell. It exits when those five folders hold no dart file at all.
//!
//! A migration of that size cannot be one commit, so the honest question at every point is not "is it done" but "did it go
//! backwards". This module answers that:
//!
//! * `leftovers` - what is still in the old folders.
//! * `regressions` - a dart file in an old folder that the pinned baseline does not name. Nothing may be *added* to a layer
//!   folder while the rest is being moved out, which is the one rule that stops the migration from treading water.
//! * `retired_baseline_entries` - a baseline entry whose file has since moved. A baseline that is never trimmed stops being
//!   evidence and becomes a list of names, so it has to shrink in the same commit that does the moving.
//! * `dangling_imports` - an import naming a file that is not there. This is the real risk of a file move: the app still
//!   builds in the editor that has it open and fails everywhere else. Nothing else in the repository would notice, because
//!   there is no dart toolchain in CI.
//! * `features` - each feature folder and which of the four inner folders it has, so a half-migrated feature is visible
//!   rather than discovered later.
//!
//! Everything is a plain library function, so the P4 worker, a future CI shard or an editor hook can call it rather than
//! reimplementing the scan.
#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The layer folders P7 empties. A dart file in any of these is work not yet done.
pub const LAYERS: [&str; 5] = ["models", "pages", "services", "widgets", "local_db"];

/// The four folders a migrated feature is split into: what it shows, what it does, where the data comes from, and the
/// shapes that data has.
pub const FEATURE_PARTS: [&str; 4] = ["presentation", "application", "data", "models"];

/// Where the pinned list of not-yet-moved files lives, relative to the app root.
pub const BASELINE: &str = "migration-baseline.txt";

/// One thing the scan found, named the way a reader would go and look at it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub file: String,
    pub detail: String,
}

fn app_root(repo: &Path) -> PathBuf {
    repo.join("apps").join("flutter")
}

/// Every dart file under the app's `lib/`, as app-relative paths, sorted.
pub fn dart_files(repo: &Path) -> Vec<String> {
    let mut found = Vec::new();
    walk(&app_root(repo).join("lib"), &app_root(repo), &mut found);
    found.sort();
    found
}

fn walk(dir: &Path, root: &Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, root, found);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("dart") {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(root) {
            found.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// The dart files still in an old layer folder: `lib/pages/x.dart` and its siblings.
pub fn leftovers(repo: &Path) -> Vec<String> {
    dart_files(repo)
        .into_iter()
        .filter(|file| {
            let rest = file.trim_start_matches("lib/");
            LAYERS
                .iter()
                .any(|layer| rest.starts_with(&format!("{layer}/")))
        })
        .collect()
}

/// The pinned baseline, one app-relative path per line. A comment line or a blank line is not an entry.
pub fn baseline(repo: &Path) -> Vec<String> {
    let path = app_root(repo).join(BASELINE);
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Files in an old folder that the baseline does not name: work that went backwards.
pub fn regressions(repo: &Path) -> Vec<String> {
    let pinned: BTreeSet<String> = baseline(repo).into_iter().collect();
    leftovers(repo)
        .into_iter()
        .filter(|file| !pinned.contains(file))
        .collect()
}

/// Baseline entries whose file has moved on. A pinned list that never shrinks is a list of names, not evidence.
pub fn retired_baseline_entries(repo: &Path) -> Vec<String> {
    let remaining: BTreeSet<String> = leftovers(repo).into_iter().collect();
    baseline(repo)
        .into_iter()
        .filter(|file| !remaining.contains(file))
        .collect()
}

/// The imports a dart file states, as the strings inside `import '...'` and `export '...'`.
///
/// Read line by line, because a directive is a whole statement: scanning for the words anywhere picks up a sentence in a
/// comment or a fragment inside a multi-line string, and reports a file the app never imported. That happened, twice, the
/// first time this gate was written.
fn stated(imports: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in imports.lines() {
        let trimmed = line.trim_start();
        let rest = match trimmed
            .strip_prefix("import ")
            .or_else(|| trimmed.strip_prefix("export "))
        {
            Some(rest) => rest,
            None => continue,
        };
        let rest = rest.trim_start();
        let Some(mark) = rest.chars().next().filter(|c| *c == '\'' || *c == '"') else {
            continue;
        };
        let Some(end) = rest[1..].find(mark) else {
            continue;
        };
        out.push(rest[1..1 + end].to_string());
    }
    out
}

/// Every import that names a file the app does not have.
///
/// Only the app's own files are checked: a `package:` import for something else (flutter, a plugin) is the pub resolver's
/// business. A `package:vimbai_mobile_client/...` import and a relative import both name a file in this tree, and after a
/// file moves one of them is wrong in a way nothing else here would catch.
pub fn dangling_imports(repo: &Path) -> Vec<Finding> {
    let root = app_root(repo).join("lib");
    // the full prefix, `package:` included. Comparing against the bare package name never matched, so every import of the
    // app's own files was skipped and the gate was blind to the one thing it exists to catch. A unit test found it.
    let package = "package:vimbai_mobile_client/";
    let mut findings = Vec::new();
    for file in dart_files(repo) {
        let path = app_root(repo).join(&file);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for import in stated(&text) {
            let target = if let Some(rest) = import.strip_prefix(package) {
                root.join(rest)
            } else if import.starts_with("dart:") || import.starts_with("package:") {
                continue;
            } else {
                let parent = path.parent().unwrap_or(&root);
                parent.join(&import)
            };
            let normal = target.to_string_lossy().replace('\\', "/");
            if normal.contains("/../") || normal.contains("/./") {
                // a path that walks up out of its own folder still has to land on a file, so it is normalised first
                let joined = target.components().fold(PathBuf::new(), |mut acc, part| {
                    match part {
                        std::path::Component::ParentDir => {
                            acc.pop();
                        }
                        std::path::Component::CurDir => {}
                        other => acc.push(other),
                    }
                    acc
                });
                if !joined.exists() {
                    findings.push(Finding {
                        file: file.clone(),
                        detail: format!("{import} does not exist"),
                    });
                }
                continue;
            }
            if !target.exists() {
                findings.push(Finding {
                    file: file.clone(),
                    detail: format!("{import} does not exist"),
                });
            }
        }
    }
    findings
}

/// Whether a folder holds a dart file anywhere below it. An empty folder is not tracked by git, so a part that is only a
/// directory is a part that disappears on a fresh clone - which is why one is not counted as migrated.
fn holds_a_dart_file(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && holds_a_dart_file(&path) {
            return true;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("dart") {
            return true;
        }
    }
    false
}

/// A feature folder and which of its four parts exist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feature {
    pub name: String,
    pub parts: Vec<String>,
}

/// Every folder under `lib/features/`, with the parts it actually has.
pub fn features(repo: &Path) -> Vec<Feature> {
    let dir = app_root(repo).join("lib").join("features");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut found: Vec<Feature> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let parts = FEATURE_PARTS
                .iter()
                .filter(|part| {
                    let dir = entry.path().join(part);
                    dir.is_dir() && holds_a_dart_file(&dir)
                })
                .map(|part| (*part).to_string())
                .collect();
            Feature { name, parts }
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Everything the gate objects to, in one list, so a failure is one report rather than five passes.
pub fn findings(repo: &Path) -> Vec<Finding> {
    let mut out: Vec<Finding> = regressions(repo)
        .into_iter()
        .map(|file| Finding {
            file,
            detail: "a dart file added to an old layer folder; it belongs under lib/features/"
                .into(),
        })
        .collect();
    out.extend(retired_baseline_entries(repo).into_iter().map(|file| Finding {
        file,
        detail: format!("named in {BASELINE} but no longer in an old layer folder; trim the pinned list"),
    }));
    out.extend(dangling_imports(repo));
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        // one unique tree per test. Naming it after the thread is not enough: threads in a pool are reused, so two tests
        // can be handed the same name and the first to finish deletes the second's tree underneath it.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let base = std::env::temp_dir().join(format!(
            "vimbai-flutter-gate-{}-{number}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("apps/flutter/lib/pages")).expect("a scratch tree");
        base
    }

    fn write(root: &Path, file: &str, body: &str) {
        let path = root.join("apps/flutter").join(file);
        std::fs::create_dir_all(path.parent().unwrap()).expect("a folder");
        std::fs::write(path, body).expect("a file");
    }

    #[test]
    fn an_import_statement_is_read_out_of_a_line() {
        let text =
            "import 'package:flutter/material.dart';\nimport \"../x.dart\";\nexport 'a.dart';";
        let stated = stated(text);
        assert!(stated.contains(&"package:flutter/material.dart".to_string()));
        assert!(stated.contains(&"../x.dart".to_string()));
        assert!(stated.contains(&"a.dart".to_string()));
    }

    #[test]
    fn a_moved_file_that_nothing_points_at_is_a_dangling_import() {
        let root = scratch();
        write(
            &root,
            "lib/features/one/presentation/a.dart",
            "import 'package:vimbai_mobile_client/pages/gone.dart';\n",
        );
        let findings = dangling_imports(&root);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].detail.contains("pages/gone.dart"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_relative_import_that_walks_out_of_its_folder_still_has_to_land() {
        let root = scratch();
        write(
            &root,
            "lib/pages/here.dart",
            "import '../../lib/pages/absent.dart';\n",
        );
        let findings = dangling_imports(&root);
        assert_eq!(findings.len(), 1, "{findings:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_added_to_an_old_layer_folder_is_a_regression() {
        let root = scratch();
        write(&root, "lib/pages/new_page.dart", "");
        write(
            &root,
            "migration-baseline.txt",
            "lib/pages/known_page.dart\n",
        );
        let findings = findings(&root);
        assert!(findings
            .iter()
            .any(|finding| finding.file == "lib/pages/new_page.dart"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_baseline_entry_that_has_moved_must_be_trimmed() {
        let root = scratch();
        write(
            &root,
            "migration-baseline.txt",
            "lib/pages/moved_away.dart\n",
        );
        let stale = retired_baseline_entries(&root);
        assert_eq!(stale, vec!["lib/pages/moved_away.dart".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_feature_reports_only_the_parts_it_has() {
        let root = scratch();
        write(&root, "lib/features/one/presentation/a.dart", "");
        write(&root, "lib/features/one/data/b.dart", "");
        std::fs::create_dir_all(root.join("apps/flutter/lib/features/one/application"))
            .expect("an empty folder");
        let found = features(&root);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].parts,
            vec!["presentation".to_string(), "data".to_string()],
            "an empty folder is not a migrated part"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
