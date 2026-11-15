#![forbid(unsafe_code)]

//! `.github/workflows` is the one part of the repository nothing local reads, which is how `ci.yml` came to need three jobs that
//! no longer existed: GitHub answers a `needs:` it cannot resolve by rejecting the whole file, so the pipeline stopped existing
//! with no error anyone saw. These tests pin the scan that reads it.

use std::path::{Path, PathBuf};

fn scratch(name: &str) -> PathBuf {
    let mut directory = std::env::temp_dir();
    directory.push(format!(
        "vimbai-workflow-references-{}-{name}",
        std::process::id()
    ));
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
fn no_workflow_names_a_job_or_action_that_is_not_there() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root")
        .to_path_buf();
    assert_eq!(
        xtask::workflows::unresolved_references(&root),
        Vec::<String>::new(),
        "a workflow names something that is not there"
    );
}

#[test]
fn a_workflow_that_needs_a_deleted_job_is_reported() {
    // exactly what was wrong with ci.yml: three jobs needing jobs that had been deleted, which GitHub answers by rejecting the
    // whole file - no run at all, and nothing local ever reads .github/workflows
    let root = scratch("workflow-needs");
    write(
        &root,
        ".github/workflows/broken.yml",
        "name: Broken\non: [push]\njobs:\n  build:\n    needs: [gone, other]\n    runs-on: ubuntu-latest\n    steps:\n      - uses: ./.github/actions/missing\n  other:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo ok\n",
    );
    write(
        &root,
        ".github/workflows/fine.yml",
        "name: Fine\non: [push]\njobs:\n  one:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo one\n  two:\n    needs:\n      - one\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo two\n",
    );
    let problems = xtask::workflows::unresolved_references(&root);
    assert_eq!(
        problems.len(),
        2,
        "one deleted job and one missing action: {problems:?}"
    );
    assert!(problems.iter().any(|problem| problem.contains("`gone`")));
    assert!(problems
        .iter()
        .any(|problem| problem.contains(".github/actions/missing")));

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}

#[test]
fn a_workflow_with_no_jobs_is_reported() {
    let root = scratch("workflow-empty");
    write(
        &root,
        ".github/workflows/empty.yml",
        "name: Empty\non: [push]\n",
    );
    let problems = xtask::workflows::unresolved_references(&root);
    assert_eq!(problems, vec!["empty.yml declares no jobs".to_string()]);

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}
