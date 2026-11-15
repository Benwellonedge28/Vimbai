#![forbid(unsafe_code)]

//! A crate the workspace does not list is never built, never linted, and never held to the safe-Rust rule. It is the same shape as
//! the other silent holes this repository has found - a job that can never start, a suite that skips - so it fails a build here.

use std::path::{Path, PathBuf};

use xtask::workspace;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root")
        .to_path_buf()
}

fn scratch(name: &str) -> PathBuf {
    let mut directory = std::env::temp_dir();
    directory.push(format!("vimbai-workspace-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

fn write(root: &Path, path: &str, contents: &str) {
    let file = root.join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).expect("a directory for the file");
    }
    std::fs::write(file, contents).expect("a written file");
}

#[test]
fn every_crate_on_disk_is_a_workspace_member() {
    let root = repository();
    assert_eq!(
        workspace::not_in_the_workspace(&root),
        Vec::<String>::new(),
        "a crate is not built by the workspace"
    );
    let crates = workspace::crates_on_disk(&root);
    assert!(
        crates.len() >= 60,
        "only {} crates were found, so the scan is not reading the workspace",
        crates.len()
    );
    let members = workspace::members(&root);
    assert_eq!(
        members.len(),
        crates.len(),
        "the members and the crates on disk disagree: {} members, {} crates",
        members.len(),
        crates.len()
    );
}

#[test]
fn a_crate_missing_from_the_members_is_reported() {
    let root = scratch("unlisted");
    write(
        &root,
        "rust/Cargo.toml",
        "[workspace]\nmembers = [\n    \"crates/common\",\n]\n",
    );
    write(
        &root,
        "rust/crates/common/Cargo.toml",
        "[package]\nname = \"common\"\n",
    );
    assert_eq!(workspace::not_in_the_workspace(&root), Vec::<String>::new());

    write(
        &root,
        "rust/crates/orphan/Cargo.toml",
        "[package]\nname = \"orphan\"\n",
    );
    let problems = workspace::not_in_the_workspace(&root);
    assert_eq!(problems.len(), 1, "one crate is unlisted: {problems:?}");
    assert!(problems[0].contains("crates/orphan"));
    assert!(
        problems[0].contains("cargo test --workspace"),
        "the message says what is lost, not only that something is wrong: {}",
        problems[0]
    );

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}

#[test]
fn a_crate_that_is_its_own_workspace_has_to_say_so() {
    let root = scratch("own");
    write(
        &root,
        "rust/Cargo.toml",
        "[workspace]\nmembers = [\"crates/common\"]\n",
    );
    write(
        &root,
        "rust/crates/common/Cargo.toml",
        "[package]\nname = \"common\"\n",
    );
    write(
        &root,
        "rust/crates/standalone/Cargo.toml",
        "[workspace]\n[package]\nname = \"standalone\"\n",
    );
    let problems = workspace::not_in_the_workspace(&root);
    assert_eq!(problems.len(), 1, "one workspace of its own: {problems:?}");
    assert!(problems[0].contains("`exclude`"));

    // said out loud, it is allowed
    write(
        &root,
        "rust/Cargo.toml",
        "[workspace]\nmembers = [\"crates/common\"]\nexclude = [\"crates/standalone\"]\n",
    );
    assert_eq!(workspace::not_in_the_workspace(&root), Vec::<String>::new());

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}

#[test]
fn a_member_with_no_crate_is_reported() {
    let root = scratch("stale");
    write(
        &root,
        "rust/Cargo.toml",
        "[workspace]\nmembers = [\n    \"crates/common\",\n    \"crates/deleted\",\n]\n",
    );
    write(
        &root,
        "rust/crates/common/Cargo.toml",
        "[package]\nname = \"common\"\n",
    );
    let problems = workspace::not_in_the_workspace(&root);
    assert_eq!(problems.len(), 1, "one member is gone: {problems:?}");
    assert!(problems[0].contains("crates/deleted"));

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}
