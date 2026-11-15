#![forbid(unsafe_code)]

//! The rules the repository keeps without a gate, so that keeping them is not a matter of memory.
//!
//! Safe Rust only, and no Python, are two standing requirements the deleted pytest suite used to enforce. Both are true today,
//! and that is exactly the state a rule stops being true in: a new integration test crate without the attribute builds, passes
//! every test it contains, and quietly brings back the ability to write `unsafe` in that file.

use std::path::{Path, PathBuf};

use xtask::safety;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root")
        .to_path_buf()
}

fn scratch(name: &str) -> PathBuf {
    // one directory per test: these run in parallel in one process, and a shared one means one test deleting another's files
    let mut directory = std::env::temp_dir();
    directory.push(format!(
        "vimbai-standing-rules-{}-{name}",
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
fn every_crate_root_forbids_unsafe_code() {
    let root = repository();
    assert_eq!(
        safety::roots_without_the_attribute(&root),
        Vec::<String>::new(),
        "a crate root does not begin with #![forbid(unsafe_code)]"
    );
    let roots = safety::crate_roots(&root);
    assert!(
        roots.len() >= 90,
        "only {} crate roots were found, so the scan is not reading the workspace",
        roots.len()
    );
    for expected in [
        "rust/apps/api-server/src/main.rs",
        "rust/crates/kernel/ledger-core/src/lib.rs",
        "rust/tests/financial-integrity/tests/invariants.rs",
    ] {
        assert!(
            roots.iter().any(|root| root.ends_with(expected)),
            "{expected} was not recognised as a crate root"
        );
    }
}

#[test]
fn a_crate_root_without_the_attribute_is_reported_including_a_test_crate() {
    let root = scratch("attribute");
    write(&root, "rust/apps/x/src/lib.rs", "pub fn one() {}\n");
    write(
        &root,
        "rust/apps/x/tests/check.rs",
        "#![forbid(unsafe_code)]\n#[test]\nfn one() {}\n",
    );
    let missing = safety::roots_without_the_attribute(&root);
    assert_eq!(
        missing.len(),
        1,
        "one crate root was written without the attribute: {missing:?}"
    );
    assert!(missing[0].contains("rust/apps/x/src/lib.rs"));

    // the test crate is a crate: its own `unsafe` would be nobody's lint, so it is held to the same rule
    write(
        &root,
        "rust/apps/x/src/lib.rs",
        "#![forbid(unsafe_code)]\npub fn one() {}\n",
    );
    write(
        &root,
        "rust/apps/x/tests/check.rs",
        "#[test]\nfn one() {}\n",
    );
    let missing = safety::roots_without_the_attribute(&root);
    assert!(
        missing
            .iter()
            .any(|problem| problem.contains("tests/check.rs")),
        "an integration test crate without the attribute went unnoticed: {missing:?}"
    );

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}

#[test]
fn real_unsafe_code_is_reported_and_discussion_of_it_is_not() {
    let root = scratch("unsafe");
    write(
        &root,
        "rust/apps/x/src/lib.rs",
        "#![forbid(unsafe_code)]\n// this crate holds no unsafe blocks, and the comment says so\nconst NOTE: &str = \"no unsafe code here\";\npub fn one() {}\n",
    );
    assert_eq!(
        safety::unsafe_that_does_something(&root),
        Vec::<String>::new(),
        "prose about unsafe code was read as unsafe code"
    );

    write(
        &root,
        "rust/apps/x/src/lib.rs",
        "#![forbid(unsafe_code)]\npub fn one() {\n    let value = unsafe { 1 };\n}\n",
    );
    let findings = safety::unsafe_that_does_something(&root);
    assert_eq!(
        findings.len(),
        1,
        "one unsafe block was written: {findings:?}"
    );
    assert!(
        findings[0].contains(":3"),
        "the finding names the line: {findings:?}"
    );

    std::fs::remove_dir_all(&root).expect("the scratch directory goes");
}

#[test]
fn python_in_the_repository_is_reported_and_absent_from_this_one() {
    let root = repository();
    assert_eq!(
        safety::python_files(&root),
        Vec::<String>::new(),
        "the repository is meant to be free of Python"
    );

    let scratch = scratch("python");
    write(&scratch, "catalog/build.py", "print('hi')\n");
    write(&scratch, "requirements-test.txt", "pytest\n");
    write(
        &scratch,
        "rust/apps/x/src/lib.rs",
        "#![forbid(unsafe_code)]\n",
    );
    let found = safety::python_files(&scratch);
    assert_eq!(found.len(), 2, "two Python files were written: {found:?}");
    assert!(found.iter().any(|problem| problem.contains("build.py")));
    assert!(found
        .iter()
        .any(|problem| problem.contains("requirements-test.txt")));

    std::fs::remove_dir_all(&scratch).expect("the scratch directory goes");
}
